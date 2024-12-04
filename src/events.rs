// Input events

use lazy_static::lazy_static;
use tokio::sync::{mpsc, Mutex, Notify};

lazy_static! {
    // We push back paginate requests into this channel, with the number
    // specifying how many messages we'd like.
    pub static ref PAGINATE_BACKWARDS: (mpsc::Sender<u16>, tokio::sync::Mutex<mpsc::Receiver<u16>>) = {
        let (tx, rx) = mpsc::channel::<u16>(10);
        (tx, Mutex::new(rx))
    };

    // We push requests to list all rooms into this channel.
    pub static ref LIST_ROOMS: (mpsc::Sender<()>, tokio::sync::Mutex<mpsc::Receiver<()>>) = {
        let (tx, rx) = mpsc::channel::<()>(10);
        (tx, Mutex::new(rx))
    };

    // e2e verification state
    pub static ref VERIFIED: Mutex<bool> = Mutex::new(false);

    // notify when verified state changes
    pub static ref VERIFIED_NOTIFY: Notify = Notify::new();
}
