//! Fire-and-forget HTTP calls for custom tiles (see `model::CustomTile`).
//! Runs off the SDL thread; the body is never read, only success/failure.
use crate::model::{CustomTile, TileMethod};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub enum Response {
    Done {
        title: String,
        result: Result<(), String>,
    },
}

pub struct Service {
    sender: mpsc::Sender<CustomTile>,
    pub receiver: mpsc::Receiver<Response>,
}

impl Service {
    pub fn new() -> Self {
        let (command_tx, command_rx) = mpsc::channel::<CustomTile>();
        let (response_tx, response_rx) = mpsc::channel::<Response>();
        thread::spawn(move || {
            let agent = agent();
            while let Ok(tile) = command_rx.recv() {
                let result = send(&agent, &tile);
                let response = Response::Done {
                    title: tile.title,
                    result,
                };
                if response_tx.send(response).is_err() {
                    break;
                }
            }
        });
        Self {
            sender: command_tx,
            receiver: response_rx,
        }
    }

    pub fn send(&self, tile: CustomTile) {
        // The worker thread only exits if its channel is gone; a dropped
        // send here just means the request is lost, not a crash.
        let _ = self.sender.send(tile);
    }
}

/// The client for tile requests, shared with the companion editor's test button.
pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(5)))
        .max_redirects(0)
        .proxy(None)
        .build()
        .new_agent()
}

pub fn send(agent: &ureq::Agent, tile: &CustomTile) -> Result<(), String> {
    match tile.method {
        TileMethod::Get => agent.get(&tile.url).call(),
        TileMethod::Post => agent.post(&tile.url).send_empty(),
    }
    .map(|_| ())
    .map_err(|error| error.to_string())
}
