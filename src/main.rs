use std::sync::Mutex;
use std::{io::Write, sync::Arc};

use anyhow::{Context, Result};
use futures_util::{pin_mut, StreamExt as _};
use matrix_sdk::{
    config::SyncSettings,
    encryption::{BackupDownloadStrategy, EncryptionSettings},
    matrix_auth::MatrixSession,
    ruma::{
        events::{
            key::verification::request::ToDeviceKeyVerificationRequestEvent,
            room::message::OriginalSyncRoomMessageEvent,
        },
        OwnedRoomId,
    },
    Client,
};
use matrix_sdk_ui::room_list_service::filters::new_filter_non_left;
use serde::{Deserialize, Serialize};
use serde_yaml;
use std::{cmp::min, path::PathBuf};
use tracing::{Event, Subscriber};
use tracing_log::LogTracer;
use tracing_subscriber::{layer::SubscriberExt, Layer, Registry};

use rooms::ROOM_LIST;

mod events;
mod keyboard;
mod rooms;
mod timeline;
mod verification;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    logfilter: String,
    homeserver_url: String,
    username: String,
    password: String,
    db_path: PathBuf,
    session_path: PathBuf,

    // sets up a timeline for this room if specified
    timeline_test_room: Option<OwnedRoomId>,

    // wait for e2e verification before constructing timeline
    #[serde(default = "default_true")]
    timeline_wait_verification: bool,
}

fn default_true() -> bool {
    true
}

async fn login(config: &Config) -> Result<Client> {
    log::info!(
        "Connecting: homeserver={} username={}",
        config.homeserver_url,
        config.username
    );

    let client = match config.session_path.exists() {
        true => {
            log::info!("Restoring login from session.");
            let client = Client::builder()
                .homeserver_url(config.homeserver_url.clone())
                .sqlite_store(config.db_path.clone(), None)
                .sliding_sync_version_builder(matrix_sdk::sliding_sync::VersionBuilder::DiscoverNative)
                .with_encryption_settings(EncryptionSettings {
                    auto_enable_cross_signing: false,
                    backup_download_strategy: BackupDownloadStrategy::AfterDecryptionFailure,
                    auto_enable_backups: false,
                })
                .build()
                .await?;

            let session_file =
                std::fs::File::open(&config.session_path).context("Unable to open session file")?;
            let session: MatrixSession =
                serde_yaml::from_reader(session_file).context("Unable to parse session file")?;
            client.restore_session(session).await?;

            client
        }
        false => {
            log::info!("Logging in with username/password.");
            let client = Client::builder()
                .homeserver_url(config.homeserver_url.clone())
                .sqlite_store(config.db_path.clone(), None)
                .with_encryption_settings(EncryptionSettings {
                    auto_enable_cross_signing: false,
                    backup_download_strategy: BackupDownloadStrategy::AfterDecryptionFailure,
                    auto_enable_backups: false,
                })
                .build()
                .await?;
            client
                .matrix_auth()
                .login_username(config.username.clone(), config.password.as_str())
                .initial_device_display_name("app-testing")
                .await?;

            let matrix_sdk::AuthSession::Matrix(session) =
                client.session().expect("Logged in client has no session!?")
            else {
                anyhow::bail!("Logged in client has no session!?");
            };
            let session_file = std::fs::File::create(&config.session_path)
                .context("Unable to create session file")?;
            serde_yaml::to_writer(session_file, &session)
                .context("Unable to write session to file")?;

            client
        }
    };

    Ok(client)
}

async fn start_matrix(config: Config, client: Client) -> Result<()> {
    client.add_event_handler(|ev: OriginalSyncRoomMessageEvent, _: Client| async move {
        let msg = format!("{}", ev.content.body().replace(|c: char| !c.is_ascii(), ""));
        log::info!("Message: {}...", &msg[0..min(60, msg.len())]);
    });

    client.add_event_handler(
        |ev: ToDeviceKeyVerificationRequestEvent, client: Client| async move {
            let request = client
                .encryption()
                .get_verification_request(&ev.sender, &ev.content.transaction_id)
                .await
                .expect("Request object wasn't created");
            tokio::spawn(verification::request_verification_handler(request));
        },
    );

    let sync_settings = SyncSettings::default();
    let sync_service = matrix_sdk_ui::sync_service::SyncService::builder(client.clone())
        .build()
        .await?;

    let mut state_sub = sync_service.state();
    tokio::spawn(async move {
        loop {
            let state = state_sub.next().await;
            match state {
                Some(state) => {
                    log::info!("sync_service state: {:?}", state);
                }
                None => {
                    log::info!("sync_service state: None");
                    break;
                }
            }
        }
    });

    let room_list_service = sync_service.room_list_service();
    let _ = tokio::spawn(watch_room_list(room_list_service));
    let _ = tokio::spawn(rooms::log_room_list());

    sync_service.start().await;

    log::info!("First sync");
    client.sync_once(sync_settings.clone()).await?;

    // if timeline_test_room is set, listen to its timeline
    if let Some(room_id) = config.timeline_test_room {
        let Some(room) = client.get_room(&room_id) else {
            anyhow::bail!("Unable to find room: {}", room_id);
        };
        let _ = tokio::spawn(timeline::watch_timeline(
            room,
            config.timeline_wait_verification,
        ));
    }

    log::info!("Sync forever");
    client.sync(sync_settings).await?;

    Ok(())
}

// Watch verification state and update global VERIFIED state.
async fn watch_verification_state(client: Client) {
    let mut sub = client.encryption().verification_state();
    loop {
        if let Some(state) = sub.next().await {
            log::info!("Received verification state update {:?}", state);
            let mut lock = events::VERIFIED.lock().await;
            match state {
                matrix_sdk::encryption::VerificationState::Verified => {
                    *lock = true;
                    events::VERIFIED_NOTIFY.notify_one();
                }
                _ => {
                    *lock = false;
                    events::VERIFIED_NOTIFY.notify_one();
                }
            }
        } else {
            break;
        }
    }
}

async fn watch_room_list(
    room_list_service: Arc<matrix_sdk_ui::RoomListService>,
) -> anyhow::Result<()> {
    log::info!("Watching room list");
    let rooms = room_list_service.all_rooms().await?;
    let (stream, controller) = rooms.entries_with_dynamic_adapters(5);
    controller.set_filter(Box::new(new_filter_non_left()));

    pin_mut!(stream);
    while let Some(diffs) = stream.next().await {
        let mut room_list = ROOM_LIST.lock().unwrap();
        for diff in diffs {
            log::info!("Room list diff: {:?}", diff.clone());
            diff.apply(&mut room_list);
        }
    }
    Ok(())
}

// When we turn on raw mode to capture keyboard input (see keyboard.start()), we
// need to be emitting carriage returns to get the logger to output lines
// properly. This is a writer for tracing that will do that.
struct CarriageReturnWriter {
    stdout: Mutex<std::io::Stdout>,
}

impl CarriageReturnWriter {
    fn new() -> Self {
        CarriageReturnWriter {
            stdout: Mutex::new(std::io::stdout()),
        }
    }
}

impl<S: Subscriber> Layer<S> for CarriageReturnWriter {
    fn on_event(&self, event: &Event, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        let metadata = event.metadata();

        let mut message = String::new();
        let mut visitor = MessageVisitor(&mut message);
        event.record(&mut visitor);

        let output = format!(
            "[{}] - {} - {}",
            metadata.target(),
            metadata.level(),
            message,
        )
        .replace("\n", "\r\n");
        let mut stdout = self.stdout.lock().unwrap();
        let _ = stdout.write(output.as_bytes());
        let _ = stdout.write("\r\n".as_bytes());
        let _ = stdout.flush();
    }
}

// Visitor to extract the message from the event
struct MessageVisitor<'a>(&'a mut String);

impl<'a> tracing::field::Visit for MessageVisitor<'a> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0.push_str(&format!("{:?}", value));
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let f = std::fs::File::open("config.yaml").context("Unable to open config.yaml")?;
    let config: Config = serde_yaml::from_reader(f)?;

    LogTracer::init().expect("Failed to set logger");
    let cr_logger = CarriageReturnWriter::new();
    let subscriber = Registry::default()
        .with(tracing_subscriber::EnvFilter::new(
            config.logfilter.as_str(),
        ))
        .with(cr_logger);
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    println!("Starting");
    println!("Use a different Matrix client to start the verification process. This app will auto-accept verification.");
    println!("");
    println!("ctrl-c -- stop program");
    println!("p -- paginate timeline backwards");
    println!("R -- list rooms");
    println!("SPACE -- print timeline");
    println!("");

    let client = login(&config).await?;

    let _ = tokio::spawn(watch_verification_state(client.clone()));

    let matrix_handle = {
        let config = config.clone();
        tokio::spawn(start_matrix(config, client))
    };

    keyboard::start().await?;

    matrix_handle.abort();
    let _ = matrix_handle.await?;
    Ok(())
}
