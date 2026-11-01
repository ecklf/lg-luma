//! The crypto price worker: one HTTP job at a time off the SDL thread, newest request wins.
use crate::crypto::{Command, Quote, parse_catalog, parse_tickers};
use std::{
    io::Read,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

const LIMIT: usize = 64 * 1024;
/// Every pair's price, for the coin search: about 3,000 entries.
const CATALOG_LIMIT: usize = 1024 * 1024;
/// Binance's global API first; the US one serves where the global API is blocked.
const HOSTS: [&str; 2] = ["https://api.binance.com", "https://api.binance.us"];

#[derive(Clone)]
pub struct Client {
    agent: ureq::Agent,
    hosts: Vec<String>,
}
impl Client {
    pub fn new(test_server: Option<&str>) -> Result<Self, String> {
        let hosts = if let Some(server) = test_server {
            let uri: ureq::http::Uri = server
                .parse()
                .map_err(|e| format!("Invalid crypto test server: {e}"))?;
            let address: SocketAddr = uri
                .authority()
                .ok_or("Crypto test server needs a loopback address and port")?
                .as_str()
                .parse()
                .map_err(|_| "Crypto tests require an IP-literal loopback address and port")?;
            if uri.scheme_str() != Some("http")
                || !address.ip().is_loopback()
                || address.port() == 0
                || uri.path() != "/"
                || uri.query().is_some()
            {
                return Err("Crypto test server must be an HTTP loopback origin".into());
            }
            vec![format!("http://{address}")]
        } else {
            HOSTS.map(String::from).to_vec()
        };
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(8)))
            .max_redirects(0)
            .proxy(None)
            .build()
            .new_agent();
        Ok(Self { agent, hosts })
    }
    pub fn quotes(&self, symbols: &[String]) -> Result<Vec<Quote>, String> {
        let pairs = format!(
            "[{}]",
            symbols
                .iter()
                .map(|symbol| format!("\"{symbol}USDT\""))
                .collect::<Vec<_>>()
                .join(",")
        );
        let mut error = String::from("Prices are unavailable.");
        for host in &self.hosts {
            let request = self
                .agent
                .get(format!("{host}/api/v3/ticker/24hr"))
                .query("symbols", &pairs)
                .query("type", "MINI");
            match Self::request(request, LIMIT) {
                Ok(value) => return parse_tickers(&value, symbols),
                Err(message) => error = message,
            }
        }
        Err(error)
    }
    /// Every coin Binance trades against USDT, for the search in Settings.
    pub fn catalog(&self) -> Result<Vec<String>, String> {
        let mut error = String::from("Coins are unavailable.");
        for host in &self.hosts {
            let request = self.agent.get(format!("{host}/api/v3/ticker/price"));
            match Self::request(request, CATALOG_LIMIT) {
                Ok(value) => return parse_catalog(&value),
                Err(message) => error = message,
            }
        }
        Err(error)
    }
    fn request(
        request: ureq::RequestBuilder<ureq::typestate::WithoutBody>,
        limit: usize,
    ) -> Result<serde_json::Value, String> {
        let mut response = request
            .header("Accept", "application/json")
            .header("User-Agent", "LumaNative")
            .call()
            .map_err(|_| "Prices are unavailable.")?;
        if !response.status().is_success() {
            return Err("Prices are unavailable.".into());
        }
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "The price response could not be read")?;
        if bytes.len() > limit {
            return Err("The price response is too large".into());
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| "The price service returned unreadable data".into())
    }
}

pub enum Response {
    Quotes {
        id: u64,
        result: Result<Vec<Quote>, String>,
    },
    Catalog(Result<Vec<String>, String>),
}

pub struct Service {
    sender: Option<mpsc::Sender<Command>>,
    catalog: Option<mpsc::Sender<()>>,
    generation: Arc<AtomicU64>,
    pub receiver: mpsc::Receiver<Response>,
}
impl Service {
    pub fn new(client: Client) -> Self {
        let (sender, jobs) = mpsc::channel::<Command>();
        let (responses, receiver) = mpsc::channel();
        let generation = Arc::new(AtomicU64::new(0));
        let current = generation.clone();
        // The coin list comes on its own worker, so a search never delays prices.
        let (catalog, wanted) = mpsc::channel::<()>();
        let catalog_client = client.clone();
        let catalog_responses = responses.clone();
        thread::spawn(move || {
            for () in wanted {
                let result = catalog_client.catalog();
                if catalog_responses.send(Response::Catalog(result)).is_err() {
                    break;
                }
            }
        });
        thread::spawn(move || {
            while let Ok(mut job) = jobs.recv() {
                // Only the newest waiting request is worth answering.
                while let Ok(newer) = jobs.try_recv() {
                    job = newer;
                }
                if current.load(Ordering::SeqCst) != job.id {
                    continue;
                }
                let result = client.quotes(&job.symbols);
                if current.load(Ordering::SeqCst) == job.id
                    && responses
                        .send(Response::Quotes { id: job.id, result })
                        .is_err()
                {
                    break;
                }
            }
        });
        Self {
            sender: Some(sender),
            catalog: Some(catalog),
            generation,
            receiver,
        }
    }
    pub fn catalog(&self) {
        if let Some(catalog) = &self.catalog {
            let _ = catalog.send(());
        }
    }
    /// Requests older than `id` are dropped before or after they run.
    pub fn generation(&self, id: u64) {
        self.generation.store(id, Ordering::SeqCst);
    }
    pub fn send(&self, command: Command) {
        self.generation(command.id);
        if let Some(sender) = &self.sender {
            let _ = sender.send(command);
        }
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.generation.store(u64::MAX, Ordering::SeqCst);
        self.sender.take();
        self.catalog.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mock_origins_are_loopback_only() {
        assert!(Client::new(Some("http://127.0.0.1:1234")).is_ok());
        assert!(Client::new(Some("http://[::1]:1234")).is_ok());
        for invalid in [
            "http://example.com:1234",
            "https://127.0.0.1:1234",
            "http://127.0.0.1:0",
            "http://127.0.0.1:1234/path",
            "http://127.0.0.1:1234?query=yes",
        ] {
            assert!(Client::new(Some(invalid)).is_err(), "{invalid}");
        }
    }
}
