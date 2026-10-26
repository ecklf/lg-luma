//! HTTPS requests for widgets, with Luma's rules for network access: rustls, no proxy, no
//! redirects, a 10 s timeout and bounded replies.
use serde_json::Value;
use std::{io::Read, time::Duration};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchError {
    /// 401 or 403: the key, if one was sent, was refused.
    Refused,
    /// 429.
    Busy,
    Unavailable(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Refused => f.write_str("refused (401/403)"),
            Self::Busy => f.write_str("too many requests (429)"),
            Self::Unavailable(why) => f.write_str(why),
        }
    }
}

/// JSON over HTTPS; tests and fixtures stand in for the network through it.
pub trait Fetch {
    fn get(
        &self,
        url: &str,
        headers: &[(String, String)],
        limit: usize,
    ) -> Result<Value, FetchError>;
}

pub struct Http {
    agent: ureq::Agent,
}

impl Default for Http {
    fn default() -> Self {
        Self::new()
    }
}

impl Http {
    pub fn new() -> Self {
        Self {
            agent: ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(10)))
                .max_redirects(0)
                .proxy(None)
                .http_status_as_error(false)
                .build()
                .new_agent(),
        }
    }

    /// The body of a GET, at most `limit` bytes.
    pub fn bytes(
        &self,
        url: &str,
        headers: &[(String, String)],
        limit: usize,
    ) -> Result<Vec<u8>, FetchError> {
        if !url.starts_with("https://") {
            return Err(FetchError::Unavailable(format!("{url}: not HTTPS")));
        }
        let mut request = self.agent.get(url).header("User-Agent", "LumaNative");
        let mut accept = false;
        for (name, value) in headers {
            accept |= name.eq_ignore_ascii_case("accept");
            request = request.header(name, value);
        }
        if !accept {
            request = request.header("Accept", "application/json");
        }
        let mut response = request
            .call()
            .map_err(|e| FetchError::Unavailable(format!("{}: {e}", strip_query(url))))?;
        match response.status().as_u16() {
            200..=299 => {}
            401 | 403 => return Err(FetchError::Refused),
            429 => return Err(FetchError::Busy),
            status => {
                return Err(FetchError::Unavailable(format!(
                    "{}: HTTP {status}",
                    strip_query(url)
                )));
            }
        }
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| FetchError::Unavailable(format!("{}: {e}", strip_query(url))))?;
        if bytes.len() > limit {
            return Err(FetchError::Unavailable(format!(
                "{}: reply too large",
                strip_query(url)
            )));
        }
        Ok(bytes)
    }
}

impl Fetch for Http {
    fn get(
        &self,
        url: &str,
        headers: &[(String, String)],
        limit: usize,
    ) -> Result<Value, FetchError> {
        let bytes = self.bytes(url, headers, limit)?;
        serde_json::from_slice(&bytes)
            .map_err(|_| FetchError::Unavailable(format!("{}: not JSON", strip_query(url))))
    }
}

/// A URL for a log line: without its query, which may carry what a user typed.
pub fn strip_query(url: &str) -> &str {
    url.split(['?', '#']).next().unwrap_or(url)
}
