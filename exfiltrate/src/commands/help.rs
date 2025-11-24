use crate::commands::COMMANDS;
use exfiltrate_internal::command::{Command, Response};

/// The remote `help` command.
///
/// Returns the full description of a requested command.
pub struct Help;

impl Command for Help {
    fn name(&self) -> &'static str {
        "help"
    }

    fn short_description(&self) -> &'static str {
        "Provides detailed help for a command.  Use this to learn more about an individual command."
    }

    fn full_description(&self) -> &'static str {
        "Provides detailed help for a command.
Usage: exfiltrate help [COMMAND]"
    }

    fn execute(&self, args: Vec<String>) -> Result<Response, Response> {
        for command in COMMANDS.lock_sync_read().iter() {
            if command.name() == args[0] {
                return Ok(command.full_description().into());
            }
        }
        Err(format!("No such command: {}", args[0]).into())
    }
}
