use crate::Result;
use crate::system::{self, System};
use serde_json::{Value, json};

/// Root-only power off for the home power button. /sbin/poweroff is the
/// command validated on the device (a real shutdown with a clean boot);
/// the tvpower luna methods were guesses that could not be confirmed.
/// Standby stays with the remote's power key, which Quick Start+ turns
/// into the suspend the wake handling covers.
pub fn dispatch(system: &impl System, action: &str) -> Result<Value> {
    system::root(system)?;
    match action {
        "shutdown" => {
            system::command(system, "/sbin/poweroff", &[])?;
            Ok(json!({"returnValue": true}))
        }
        other => Err(format!("Unknown power action: {other}")),
    }
}
