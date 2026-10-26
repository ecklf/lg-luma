//! Entering a long key from a phone: while its dialog is open, Luma serves one page on the home
//! network at a one-time address shown as a QR code. The page takes the key, hands it to the SDL
//! thread and the server stops. Only that address answers; bodies and keys are bounded.
use crate::app_artwork::Artwork;
use std::{
    io::{Read, Write},
    net::{IpAddr, SocketAddr, TcpListener, TcpStream, UdpSocket},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

const PORTS: std::ops::RangeInclusive<u16> = 8765..=8775;
const REQUEST_LIMIT: usize = 8 * 1024;
const KEY_LIMIT: usize = 256;

/// A rule a key must pass, e.g. a store widget's pattern.
pub type Accepts = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// The phone page's words, in Luma's language.
#[derive(Clone)]
pub struct PageTexts {
    pub title: String,
    pub steps: Vec<String>,
    pub field: String,
    pub send: String,
    pub saved: String,
    pub invalid: String,
    /// A store widget's own rule for its secret; without one, a key of letters, digits,
    /// `-`, `_` and `.` up to 128 characters.
    pub accepts: Option<Accepts>,
}

/// A running one-time server; dropping it stops the server.
pub struct PhoneKey {
    pub url: String,
    pub qr: Arc<Artwork>,
    receiver: mpsc::Receiver<String>,
    stop: Arc<AtomicBool>,
}

impl PhoneKey {
    pub fn start(texts: PageTexts) -> Result<Self, String> {
        let address = lan_address().ok_or("The TV has no network address")?;
        let listener = PORTS
            .clone()
            .find_map(|port| TcpListener::bind(SocketAddr::new(address, port)).ok())
            .ok_or("No free port for the phone page")?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let token = token();
        let url = format!("http://{address}:{port}/{token}");
        let pixels = qr_image(&url)?;
        let qr = Arc::new(Artwork {
            key: format!("qr:{token}"),
            pixels,
            backdrop: [[255; 3]; 2],
        });
        let (sender, receiver) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let running = stop.clone();
        thread::spawn(move || {
            while !running.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if let Some(key) = serve(stream, &token, &texts) {
                            let _ = sender.send(key);
                            break;
                        }
                    }
                    Err(_) => thread::sleep(Duration::from_millis(60)),
                }
            }
        });
        Ok(Self {
            url,
            qr,
            receiver,
            stop,
        })
    }

    /// The key once the phone sent one.
    pub fn poll(&self) -> Option<String> {
        self.receiver.try_recv().ok()
    }
}

impl Drop for PhoneKey {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// The address other devices on the home network reach the TV at. Connecting a UDP socket
/// sends nothing; it only picks the interface of the default route.
fn lan_address() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let address = socket.local_addr().ok()?.ip();
    (!address.is_loopback() && !address.is_unspecified()).then_some(address)
}

fn token() -> String {
    let mut bytes = [0u8; 8];
    let random = std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .is_ok();
    if !random {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        bytes = (nanos as u64 ^ (std::process::id() as u64) << 32).to_le_bytes();
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn serve(mut stream: TcpStream, token: &str, texts: &PageTexts) -> Option<String> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    let mut request = Vec::new();
    let mut chunk = [0u8; 2048];
    // Read the headers, then the declared body, up to the limit.
    while request.len() < REQUEST_LIMIT {
        let Ok(read) = stream.read(&mut chunk) else {
            break;
        };
        if read == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..read]);
        if let Some(end) = find(&request, b"\r\n\r\n") {
            let length = content_length(&request[..end]);
            if request.len() >= end + 4 + length {
                break;
            }
        }
    }
    let (reply, key) = respond(&request, token, texts);
    let _ = stream.write_all(&reply);
    key
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn content_length(headers: &[u8]) -> usize {
    String::from_utf8_lossy(headers)
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().ok())?
        })
        .unwrap_or(0)
        .min(REQUEST_LIMIT)
}

/// The reply to one request and the key it carried, if any.
pub fn respond(request: &[u8], token: &str, texts: &PageTexts) -> (Vec<u8>, Option<String>) {
    let text = String::from_utf8_lossy(request);
    let line = text.lines().next().unwrap_or("");
    let mut parts = line.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    if path.trim_start_matches('/') != token || token.is_empty() {
        return (http("404 Not Found", "Not found"), None);
    }
    match method {
        "GET" => (http("200 OK", &page(texts, None)), None),
        "POST" => {
            let body = text.split_once("\r\n\r\n").map_or("", |(_, body)| body);
            let key = body
                .split('&')
                .find_map(|pair| pair.strip_prefix("key="))
                .map(decode)
                .map(|key| key.trim().to_owned())
                .unwrap_or_default();
            let valid = !key.is_empty()
                && key.len() <= KEY_LIMIT
                && match &texts.accepts {
                    Some(accepts) => accepts(&key),
                    None => {
                        key.len() <= 128
                            && key
                                .chars()
                                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
                    }
                };
            if valid {
                (http("200 OK", &page(texts, Some(&texts.saved))), Some(key))
            } else if key.is_empty() {
                (http("200 OK", &page(texts, None)), None)
            } else {
                (http("200 OK", &page(texts, Some(&texts.invalid))), None)
            }
        }
        _ => (http("405 Method Not Allowed", "Method not allowed"), None),
    }
}

/// `application/x-www-form-urlencoded` decoding: `+` is a space, `%XX` a byte.
fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let hex = bytes
            .get(index + 1..index + 3)
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match (bytes[index], hex) {
            (b'+', _) => out.push(b' '),
            (b'%', Some(byte)) => {
                out.push(byte);
                index += 2;
            }
            (byte, _) => out.push(byte),
        }
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn page(texts: &PageTexts, message: Option<&str>) -> String {
    let steps: String = texts
        .steps
        .iter()
        .map(|step| format!("<li>{}</li>", escape(step)))
        .collect();
    let message = message.map_or(String::new(), |m| {
        format!("<p class=\"m\">{}</p>", escape(m))
    });
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Luma</title>\
<style>body{{font:17px -apple-system,system-ui,sans-serif;margin:0;padding:24px;background:#12161b;color:#eee}}\
h1{{font-size:22px}}li{{margin:8px 0}}input{{width:100%;box-sizing:border-box;font-size:17px;padding:14px;\
border-radius:12px;border:1px solid #444;background:#1d2329;color:#fff}}button{{margin-top:14px;width:100%;\
font-size:18px;padding:14px;border:0;border-radius:12px;background:#f0f0f0;color:#111;font-weight:600}}\
.m{{padding:12px;border-radius:12px;background:#24303a}}</style></head><body>\
<h1>{}</h1>{message}<ol>{steps}</ol><form method=\"post\"><input name=\"key\" autocomplete=\"off\" \
autocapitalize=\"off\" spellcheck=\"false\" placeholder=\"{}\"><button>{}</button></form></body></html>",
        escape(&texts.title),
        escape(&texts.field),
        escape(&texts.send),
    )
}

fn http(status: &str, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
Cache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

/// The QR code of `text`, 8 pixels per module with a 4-module white border.
pub fn qr_image(text: &str) -> Result<image::RgbaImage, String> {
    let code = qrcode::QrCode::new(text.as_bytes()).map_err(|e| e.to_string())?;
    let width = code.width() as u32;
    let colors = code.to_colors();
    let (scale, border) = (8u32, 4u32);
    let size = (width + 2 * border) * scale;
    Ok(image::RgbaImage::from_fn(size, size, |x, y| {
        let (mx, my) = (x / scale, y / scale);
        let dark = mx >= border
            && my >= border
            && mx < width + border
            && my < width + border
            && colors[((my - border) * width + (mx - border)) as usize] == qrcode::Color::Dark;
        if dark {
            image::Rgba([0, 0, 0, 255])
        } else {
            image::Rgba([255, 255, 255, 255])
        }
    }))
}

#[cfg(test)]
#[path = "phone-key-tests.rs"]
mod tests;
