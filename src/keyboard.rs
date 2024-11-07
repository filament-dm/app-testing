use crossterm::event::KeyModifiers;
use crossterm::{
    event::{read, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode},
};

use crate::events::PAGINATE_BACKWARDS;

async fn process_events() -> anyhow::Result<()> {
    loop {
        let Event::Key(event) = read()? else {
            log::error!("Failed to read input event");
            break;
        };

        if event.code == KeyCode::Char('c') && event.modifiers.contains(KeyModifiers::CONTROL) {
            break;
        }

        if event.code == KeyCode::Enter {
            println!("\r");
        } else if event.code == KeyCode::Char('p') {
            let _ = PAGINATE_BACKWARDS.0.send(10).await;
        } else if event.code == KeyCode::Char(' ') {
            let _ = PAGINATE_BACKWARDS.0.send(0).await;
        }
    }

    Ok(())
}

/// Starts the input event loop.
pub async fn start() -> anyhow::Result<()> {
    enable_raw_mode()?;
    process_events().await?;
    disable_raw_mode()?;

    Ok(())
}
