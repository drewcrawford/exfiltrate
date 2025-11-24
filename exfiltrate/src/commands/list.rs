use crate::command::{Command, Response};
use crate::commands::COMMANDS;
use exfiltrate_internal::commands::list::ListItem;

/// The remote `list` command.
///
/// Returns a list of all available commands registered in the application.
pub(crate) struct List;

impl Command for List {
    fn name(&self) -> &'static str {
        "list"
    }
    fn short_description(&self) -> &'static str {
        "List the currently available commands.  Use this command to list currently available commands, which change depending on whether or not the debugged program is currently running."
    }

    fn full_description(&self) -> &'static str {
        "List all available commands.

This CLI program debugs a remote application.  Some commands are only available when the remote application is running.
        "
    }
    fn execute(&self, _args: Vec<String>) -> Result<Response, Response> {
        let mut items = Vec::new();

        for command in COMMANDS.lock_sync_read().iter() {
            let item = ListItem {
                name: command.name().to_string(),
                short_description: command.short_description().to_string(),
            };
            items.push(item);
        }
        Response::from_serialize(&items)
    }
}
