//! Privileged, bounded TV operations. Shared with Luma for its already-root Home process.
//! No SDL, image decoding, browser engine, or persistent daemon is needed by the helper.
pub mod files;
pub mod home;
pub mod input;
pub mod system;
pub mod uninstall;

pub type Result<T> = std::result::Result<T, String>;
pub const APP: &str = "com.ecklf.luma";
pub const ROOTS: [&str; 5] = [
    "/media/developer/apps/usr/palm/applications",
    "/media/cryptofs/apps/usr/palm/applications",
    "/usr/palm/applications",
    "/media/system/apps/usr/palm/applications",
    "/mnt/otncabi/usr/palm/applications",
];
pub fn valid_app_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .split(['.', '_', '-'])
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric()))
}
pub fn valid_background(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
pub fn accepted(reply: serde_json::Value) -> Result<serde_json::Value> {
    if reply
        .get("returnValue")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        Ok(reply)
    } else {
        Err(format!("TV service refused: {reply}"))
    }
}

#[cfg(test)]
mod tests;
