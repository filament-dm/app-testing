use anyhow::Result;
use matrix_sdk::{config::SyncSettings, Client};

async fn login() -> Result<Client> {
    let homeserver_url = "http://filament-dev.local:8448";
    let username = "test2";
    let password = "foobar123foobar";

    let builder = Client::builder().homeserver_url(homeserver_url);
    let client = builder.build().await?;
    client
        .matrix_auth()
        .login_username(username, password)
        .initial_device_display_name("sync-testing")
        .await?;

    Ok(client)
}

#[tokio::main]
async fn main() -> Result<()> {
    simple_logger::SimpleLogger::new().env().init().unwrap();

    // let room_id = String::from("foo");
    let client = login().await?;

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
                    println!("sync_service state: {:?}", state);
                }
                None => {
                    println!("sync_service state: None");
                    break;
                }
            }
        }
    });
    sync_service.start().await;

    println!("first sync");
    client.sync_once(sync_settings.clone()).await?;

    println!("sync forever");
    client.sync(sync_settings).await?;

    Ok(())
}
