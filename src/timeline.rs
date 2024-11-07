use std::sync::Arc;

use anyhow::Result;
use futures_util::StreamExt;
use matrix_sdk::Room;
use matrix_sdk_ui::{
    eyeball_im::VectorDiff,
    timeline::{self, RoomExt, TimelineEventItemId, TimelineItem, TimelineItemContent},
};
use tokio::sync::Mutex;

use crate::events::{self, PAGINATE_BACKWARDS};

async fn wait_verified() {
    loop {
        let verification_state = {
            let lock = events::VERIFIED.lock().await;
            *lock
        };

        if verification_state {
            break;
        } else {
            events::VERIFIED_NOTIFY.notified().await;
        }
    }
}

pub async fn watch_timeline(room: Room, wait_for_verification: bool) -> Result<()> {
    if wait_for_verification {
        wait_verified().await;
    }

    log::info!("Watching timeline for room: {}", room.room_id());
    let timeline = room.timeline().await?;
    let (timeline_items, mut timeline_stream) = timeline.subscribe_batched().await;
    let timeline_items = Arc::new(Mutex::new(timeline_items));

    {
        let timeline_items = timeline_items.clone();
        tokio::spawn(async move {
            while let Some(diffs) = timeline_stream.next().await {
                let mut items = timeline_items.lock().await;
                for diff in diffs {
                    // log::info!("Timeline diff: {:?}", diff);
                    match diff {
                        VectorDiff::Append { values } => {
                            log::debug!("VectorDiff::Append");
                            items.extend(values);
                        }
                        VectorDiff::Clear => {
                            log::debug!("VectorDiff::Clear");
                            items.clear();
                        }
                        VectorDiff::PushFront { value } => {
                            log::debug!("VectorDiff::PushFront");
                            items.push_front(value);
                        }
                        VectorDiff::PushBack { value } => {
                            log::debug!("VectorDiff::PushBack");
                            items.push_back(value);
                        }
                        VectorDiff::PopFront => {
                            log::debug!("VectorDiff::PopFront");
                            items.pop_front();
                        }
                        VectorDiff::PopBack => {
                            log::debug!("VectorDiff::PopBack");
                            items.pop_back();
                        }
                        VectorDiff::Insert { index, value } => {
                            log::debug!("VectorDiff::Insert");
                            items.insert(index, value);
                        }
                        VectorDiff::Set { index, value } => {
                            log::debug!("VectorDiff::Set");
                            items[index] = value;
                        }
                        VectorDiff::Remove { index } => {
                            log::debug!("VectorDiff::Remove");
                            items.remove(index);
                        }
                        VectorDiff::Truncate { length, .. } => {
                            log::debug!("VectorDiff::Truncate");
                            items.truncate(length);
                        }
                        VectorDiff::Reset { values } => {
                            log::debug!("VectorDiff::Reset");
                            items.clear();
                            items.extend(values);
                        }
                    }
                }
            }
        });
    }

    {
        let timeline_items = timeline_items.clone();
        tokio::spawn(async move {
            loop {
                let mut pb = PAGINATE_BACKWARDS.1.lock().await;
                let Some(amt) = pb.recv().await else {
                    continue;
                };
                if amt > 0 {
                    let _ = timeline.paginate_backwards(amt).await;
                }

                log::info!("Timeline:");
                let items = timeline_items.lock().await;
                for item in items.iter() {
                    match display(item) {
                        Some(s) => log::info!("{}", s),
                        None => continue,
                    }
                }
                log::info!("");
            }
        });
    }

    Ok(())
}

// Formats a timeline item as a string for display.
fn display(item: &TimelineItem) -> Option<String> {
    match item.kind() {
        timeline::TimelineItemKind::Event(event) => {
            let event_id = match event.identifier() {
                TimelineEventItemId::EventId(id) => id.to_string(),
                TimelineEventItemId::TransactionId(id) => id.to_string(),
            };
            let body = match event.content() {
                TimelineItemContent::Message(msg) => msg.body(),
                TimelineItemContent::UnableToDecrypt(_) => "Unable to decrypt",
                _ => "---",
            };
            Some(format!("{}: {}", event_id, body).to_string())
        }
        timeline::TimelineItemKind::Virtual(_) => None,
    }
}
