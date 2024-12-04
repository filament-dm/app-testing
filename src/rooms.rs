use std::sync::Mutex;

use lazy_static::lazy_static;
use matrix_sdk_ui::eyeball_im::Vector;
use matrix_sdk_ui::room_list_service::Room;

use crate::events::LIST_ROOMS;

lazy_static! {
    pub static ref ROOM_LIST: Mutex<Vector<Room>> = Mutex::new(Vector::new());
}

pub async fn log_room_list() {
    loop {
        let mut rl = LIST_ROOMS.1.lock().await;
        let Some(_) = rl.recv().await else {
            continue;
        };

        let room_list = ROOM_LIST.lock().unwrap();
        log::info!("Current room list:");
        for room in room_list.iter() {
            let name = match room.cached_display_name() {
                Some(name) => name,
                None => room.id().to_string(),
            };
            let unread_count = room.unread_notification_counts();
            log::info!("  {} ({})", name, unread_count.notification_count);
        }
    }
}
