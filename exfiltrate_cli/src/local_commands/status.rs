// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::wire::client::CLIENT;
use exfiltrate_internal::command::{Command, Response};

/// The local `status` command.
///
/// Checks and reports the current connection status to the remote application.
pub struct Status;

impl Command for Status {
    fn name(&self) -> &'static str {
        "status"
    }
    fn short_description(&self) -> &'static str {
        "Displays if the debugged application is connected or not; use this to determine if it's running."
    }
    fn full_description(&self) -> &'static str {
        "Displays if the debugged application is connected or not.

exfiltrate is designed to debug a remote application, but it may be crashed or not running.
This tool can detect if we can connect.
        "
    }
    fn execute(&self, _args: Vec<String>) -> Result<Response, Response> {
        match CLIENT.as_ref() {
            Ok(_client) => Ok("Status: connected".into()),
            Err(e) => Ok(format!("Status: failed ({})", e).into()),
        }
    }
}
