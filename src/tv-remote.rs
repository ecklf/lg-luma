//! Whole-TV remote control for the companion page, through the TV's own remote-app interface
//! (SSAP on ws://127.0.0.1:3000), the same way LG's remote apps work.
//!
//! The first connection asks the TV to show its "Allow?" prompt, which someone has to accept with
//! the TV remote; the TV then hands out a client key that is kept next to Luma's preferences. With
//! it, buttons reach whatever app is in front, and volume, power and text input work too. Nothing
//! here runs unless the owner turns "Control the whole TV" on.
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    path::Path,
    sync::mpsc,
    thread,
    time::Duration,
};

const ADDRESS: &str = "127.0.0.1:3000";
const TIMEOUT: Duration = Duration::from_secs(5);
/// How long the TV's Allow prompt may wait for someone at the TV.
const PROMPT_TIME: Duration = Duration::from_secs(60);
/// Only what the companion page uses.
const PERMISSIONS: [&str; 7] = [
    "CONTROL_AUDIO",
    "CONTROL_INPUT_MEDIA_PLAYBACK",
    "CONTROL_INPUT_TEXT",
    "CONTROL_MOUSE_AND_KEYBOARD",
    "CONTROL_POWER",
    "LAUNCH",
    "READ_RUNNING_APPS",
];
/// The page's buttons and the TV remote button names they press.
pub const BUTTONS: [(&str, &str); 17] = [
    ("up", "UP"),
    ("down", "DOWN"),
    ("left", "LEFT"),
    ("right", "RIGHT"),
    ("ok", "ENTER"),
    ("back", "BACK"),
    ("home", "HOME"),
    ("menu", "MENU"),
    ("exit", "EXIT"),
    ("volume_up", "VOLUMEUP"),
    ("volume_down", "VOLUMEDOWN"),
    ("mute", "MUTE"),
    ("play", "PLAY"),
    ("pause", "PAUSE"),
    ("rewind", "REWIND"),
    ("forward", "FASTFORWARD"),
    ("info", "INFO"),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Off, or not tried yet.
    #[default]
    Off,
    /// The TV shows its Allow prompt.
    Confirm,
    Ready,
    Denied,
    /// This TV has no remote-app interface Luma can reach.
    Unavailable,
}

pub enum Command {
    Enable(bool),
    Button(&'static str),
    Text(String),
    Enter,
    PowerOff,
}

pub struct Service {
    sender: mpsc::Sender<Command>,
    pub receiver: mpsc::Receiver<Status>,
    enabled: bool,
}

impl Service {
    pub fn new(preferences: &Path, available: bool) -> Self {
        let key_path = preferences.with_file_name("tv-remote-key");
        let (sender, commands) = mpsc::channel();
        let (status, receiver) = mpsc::channel();
        let _ = thread::Builder::new()
            .name("luma-tv-remote".into())
            .spawn(move || worker(&key_path, available, &commands, &status));
        Self {
            sender,
            receiver,
            enabled: false,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.enabled = enabled;
            let _ = self.sender.send(Command::Enable(enabled));
        }
    }

    pub fn send(&self, command: Command) {
        let _ = self.sender.send(command);
    }
}

pub fn button(name: &str) -> Option<&'static str> {
    BUTTONS
        .iter()
        .find(|(page, _)| *page == name)
        .map(|(_, tv)| *tv)
}

fn worker(
    key_path: &Path,
    available: bool,
    commands: &mpsc::Receiver<Command>,
    status: &mpsc::Sender<Status>,
) {
    let mut session: Option<Session> = None;
    let mut enabled = false;
    while let Ok(command) = commands.recv() {
        let command = match command {
            Command::Enable(on) => {
                enabled = on;
                session = None;
                let next = if !on {
                    Status::Off
                } else if !available {
                    Status::Unavailable
                } else {
                    // Pairing happens now, so the Allow prompt appears while the owner is
                    // looking at the switch they just turned on.
                    match Session::open(key_path, status) {
                        Ok(open) => {
                            session = Some(open);
                            Status::Ready
                        }
                        Err(Failure::Denied) => Status::Denied,
                        Err(Failure::Other(error)) => {
                            eprintln!("Luma TV remote: {error}");
                            Status::Unavailable
                        }
                    }
                };
                if status.send(next).is_err() {
                    break;
                }
                continue;
            }
            command => command,
        };
        if !enabled || !available {
            continue;
        }
        // A dropped connection (TV standby, interface restart, a quiet connection the TV closed)
        // reconnects with the saved key.
        for attempt in 0..2 {
            if session.is_none() {
                match Session::open(key_path, status) {
                    Ok(open) => session = Some(open),
                    Err(error) => {
                        eprintln!("Luma TV remote: {}", error.text());
                        break;
                    }
                }
            }
            let Some(open) = session.as_mut() else { break };
            match open.run(&command) {
                Ok(()) => break,
                Err(error) => {
                    session = None;
                    if attempt == 1 {
                        eprintln!("Luma TV remote: {error}");
                    }
                }
            }
        }
    }
}

enum Failure {
    Denied,
    Other(String),
}

impl Failure {
    fn text(&self) -> String {
        match self {
            Self::Denied => "denied on the TV".into(),
            Self::Other(error) => error.clone(),
        }
    }
}

impl From<String> for Failure {
    fn from(error: String) -> Self {
        Self::Other(error)
    }
}

struct Session {
    control: Socket,
    /// Where button presses go; opened right after registration.
    pointer: Option<Socket>,
    next_id: u64,
}

impl Session {
    fn open(key_path: &Path, status: &mpsc::Sender<Status>) -> Result<Self, Failure> {
        let mut control = Socket::connect(ADDRESS, "/")?;
        let saved = std::fs::read_to_string(key_path).ok();
        let mut payload = serde_json::json!({
            "forcePairing": false,
            "pairingType": "PROMPT",
            "manifest": {
                "manifestVersion": 1,
                "appVersion": "1.1",
                "permissions": PERMISSIONS,
            },
        });
        if let Some(key) = saved
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())
        {
            payload["client-key"] = serde_json::json!(key);
        }
        control.send_text(
            &serde_json::json!({ "type": "register", "id": "register", "payload": payload })
                .to_string(),
        )?;
        control
            .stream
            .set_read_timeout(Some(PROMPT_TIME))
            .map_err(|e| e.to_string())?;
        loop {
            let message: serde_json::Value =
                serde_json::from_str(&control.read_text()?).map_err(|e| e.to_string())?;
            match message["type"].as_str() {
                Some("registered") => {
                    if let Some(key) = message["payload"]["client-key"].as_str()
                        && saved.as_deref().map(str::trim) != Some(key)
                    {
                        save_key(key_path, key)?;
                    }
                    break;
                }
                // The TV is showing its Allow prompt.
                Some("response") if message["payload"]["pairingType"] == "PROMPT" => {
                    let _ = status.send(Status::Confirm);
                }
                Some("error") => {
                    let error = message["error"].as_str().unwrap_or_default();
                    return Err(if error.contains("denied") || error.contains("403") {
                        Failure::Denied
                    } else {
                        Failure::Other(error.into())
                    });
                }
                _ => {}
            }
        }
        control
            .stream
            .set_read_timeout(Some(TIMEOUT))
            .map_err(|e| e.to_string())?;
        let mut session = Self {
            control,
            pointer: None,
            next_id: 0,
        };
        let reply = session.request(
            "ssap://com.webos.service.networkinput/getPointerInputSocket",
            None,
        )?;
        let socket = reply["payload"]["socketPath"]
            .as_str()
            .ok_or_else(|| Failure::Other("No pointer input socket".into()))?;
        session.pointer = Some(Socket::connect(ADDRESS, &socket_path(socket))?);
        Ok(session)
    }

    fn request(
        &mut self,
        uri: &str,
        payload: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        self.next_id += 1;
        let id = format!("luma-{}", self.next_id);
        let mut message = serde_json::json!({ "type": "request", "id": id, "uri": uri });
        if let Some(payload) = payload {
            message["payload"] = payload;
        }
        self.control.send_text(&message.to_string())?;
        loop {
            let reply: serde_json::Value =
                serde_json::from_str(&self.control.read_text()?).map_err(|e| e.to_string())?;
            if reply["id"] == id.as_str() {
                if reply["type"] == "error" {
                    return Err(reply["error"].as_str().unwrap_or("Request failed").into());
                }
                return Ok(reply);
            }
        }
    }

    fn run(&mut self, command: &Command) -> Result<(), String> {
        let pointer_open = self.pointer.as_ref().is_some_and(Socket::idle_and_open);
        if !self.control.idle_and_open() || !pointer_open {
            return Err("Remote interface connection went stale".into());
        }
        match command {
            Command::Button(name) => self
                .pointer
                .as_mut()
                .ok_or("No pointer input socket")?
                .send_text(&format!("type:button\nname:{name}\n\n")),
            Command::Text(text) => self
                .request(
                    "ssap://com.webos.service.ime/insertText",
                    Some(serde_json::json!({ "text": text, "replace": 0 })),
                )
                .map(drop),
            Command::Enter => self
                .request("ssap://com.webos.service.ime/sendEnterKey", None)
                .map(drop),
            Command::PowerOff => self.request("ssap://system/turnOff", None).map(drop),
            Command::Enable(_) => Ok(()),
        }
    }
}

/// The pointer socket's path on the local interface; the TV may name its network address or
/// the TLS port, but locally the plain port serves the same path.
fn socket_path(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    match rest.find('/') {
        Some(index) => rest[index..].to_owned(),
        None => "/".into(),
    }
}

fn save_key(path: &Path, key: &str) -> Result<(), String> {
    crate::storage::write_atomic(path, key.as_bytes())
}

/// A minimal WebSocket client: text frames, masked as clients must, with ping replies.
struct Socket {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
}

impl Socket {
    fn connect(address: &str, path: &str) -> Result<Self, String> {
        let stream = TcpStream::connect_timeout(
            &address.parse().map_err(|_| "Invalid address".to_owned())?,
            TIMEOUT,
        )
        .map_err(|e| format!("Remote interface unavailable: {e}"))?;
        stream
            .set_read_timeout(Some(TIMEOUT))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(TIMEOUT))
            .map_err(|e| e.to_string())?;
        let mut key = [0; 16];
        getrandom::getrandom(&mut key).map_err(|e| e.to_string())?;
        let mut socket = Self {
            reader: BufReader::new(stream.try_clone().map_err(|e| e.to_string())?),
            stream,
        };
        write!(
            socket.stream,
            "GET {path} HTTP/1.1\r\nHost: {address}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {}\r\nSec-WebSocket-Version: 13\r\n\r\n",
            base64(&key)
        )
        .map_err(|e| e.to_string())?;
        let mut status = String::new();
        socket
            .reader
            .read_line(&mut status)
            .map_err(|e| e.to_string())?;
        if !status.starts_with("HTTP/1.1 101") {
            return Err(format!("Remote interface refused: {}", status.trim()));
        }
        loop {
            let mut line = String::new();
            if socket
                .reader
                .read_line(&mut line)
                .map_err(|e| e.to_string())?
                == 0
            {
                return Err("Remote interface closed".into());
            }
            if line == "\r\n" {
                break;
            }
        }
        Ok(socket)
    }

    fn send_text(&mut self, text: &str) -> Result<(), String> {
        self.send_frame(0x1, text.as_bytes())
    }

    /// Whether the idle connection is still open with nothing unread. The TV closes a quiet
    /// connection after a while (it pings, and nobody reads to answer between presses), and a
    /// write to a closed socket still succeeds once, so the press would vanish. Anything waiting
    /// (a ping, a close, a late reply) also means starting over is simpler than catching up.
    fn idle_and_open(&self) -> bool {
        if !self.reader.buffer().is_empty() || self.stream.set_nonblocking(true).is_err() {
            return false;
        }
        let quiet = matches!(
            self.stream.peek(&mut [0; 1]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
        );
        self.stream.set_nonblocking(false).is_ok() && quiet
    }

    fn send_frame(&mut self, opcode: u8, payload: &[u8]) -> Result<(), String> {
        let frame = frame(opcode, payload, random_mask()?);
        self.stream.write_all(&frame).map_err(|e| e.to_string())
    }

    fn read_text(&mut self) -> Result<String, String> {
        let mut message = Vec::new();
        loop {
            let mut head = [0; 2];
            self.reader
                .read_exact(&mut head)
                .map_err(|e| e.to_string())?;
            let fin = head[0] & 0x80 != 0;
            let opcode = head[0] & 0x0f;
            let masked = head[1] & 0x80 != 0;
            let length = match head[1] & 0x7f {
                126 => {
                    let mut bytes = [0; 2];
                    self.reader
                        .read_exact(&mut bytes)
                        .map_err(|e| e.to_string())?;
                    u64::from(u16::from_be_bytes(bytes))
                }
                127 => {
                    let mut bytes = [0; 8];
                    self.reader
                        .read_exact(&mut bytes)
                        .map_err(|e| e.to_string())?;
                    u64::from_be_bytes(bytes)
                }
                length => u64::from(length),
            };
            if length > 1 << 20 {
                return Err("Remote interface message too large".into());
            }
            let mut mask = [0; 4];
            if masked {
                self.reader
                    .read_exact(&mut mask)
                    .map_err(|e| e.to_string())?;
            }
            let mut payload = vec![0; length as usize];
            self.reader
                .read_exact(&mut payload)
                .map_err(|e| e.to_string())?;
            if masked {
                for (index, byte) in payload.iter_mut().enumerate() {
                    *byte ^= mask[index % 4];
                }
            }
            match opcode {
                0x8 => return Err("Remote interface closed".into()),
                0x9 => self.send_frame(0xA, &payload)?,
                0xA => {}
                0x0 | 0x1 => {
                    message.extend_from_slice(&payload);
                    if fin {
                        return String::from_utf8(message).map_err(|e| e.to_string());
                    }
                }
                _ => {}
            }
        }
    }
}

fn random_mask() -> Result<[u8; 4], String> {
    let mut mask = [0; 4];
    getrandom::getrandom(&mut mask).map_err(|e| e.to_string())?;
    Ok(mask)
}

/// One final, masked client frame.
fn frame(opcode: u8, payload: &[u8], mask: [u8; 4]) -> Vec<u8> {
    let mut frame = vec![0x80 | opcode];
    match payload.len() {
        length @ 0..=125 => frame.push(0x80 | length as u8),
        length @ 126..=0xffff => {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(length as u16).to_be_bytes());
        }
        length => {
            frame.push(0x80 | 127);
            frame.extend_from_slice(&(length as u64).to_be_bytes());
        }
    }
    frame.extend_from_slice(&mask);
    frame.extend(
        payload
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask[index % 4]),
    );
    frame
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let value = chunk.iter().enumerate().fold(0u32, |value, (index, byte)| {
            value | (u32::from(*byte) << (16 - 8 * index))
        });
        for index in 0..4 {
            if index <= chunk.len() {
                out.push(ALPHABET[(value >> (18 - 6 * index)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "tv-remote-tests.rs"]
mod tests;
