//! Fire-and-forget HTTP calls for custom tiles (see `model::CustomTile`).
//! Runs off the SDL thread; the body is never read, only success/failure.
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub enum Response {
    Done { result: Result<(), String> },
}

pub struct Service {
    sender: mpsc::Sender<String>,
    pub receiver: mpsc::Receiver<Response>,
}

impl Service {
    pub fn new() -> Self {
        let (command_tx, command_rx) = mpsc::channel::<String>();
        let (response_tx, response_rx) = mpsc::channel::<Response>();
        thread::spawn(move || {
            let agent = ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(5)))
                .max_redirects(0)
                .proxy(None)
                .build()
                .new_agent();
            while let Ok(url) = command_rx.recv() {
                let result = agent
                    .get(&url)
                    .call()
                    .map(|_| ())
                    .map_err(|error| error.to_string());
                if response_tx.send(Response::Done { result }).is_err() {
                    break;
                }
            }
        });
        Self {
            sender: command_tx,
            receiver: response_rx,
        }
    }

    pub fn send(&self, url: String) {
        // The worker thread only exits if its channel is gone; a dropped
        // send here just means the request is lost, not a crash.
        let _ = self.sender.send(url);
    }
}
