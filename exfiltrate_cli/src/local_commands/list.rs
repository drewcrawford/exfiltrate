// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::wire::client::CLIENT;
use exfiltrate_internal::command::{Command, Response};
use exfiltrate_internal::commands::list::ListItem;
use exfiltrate_internal::rpc::{CommandInvocation, RPC};
use std::collections::HashSet;

/// The local `list` command.
///
/// This command aggregates available commands from two sources:
/// 1.  **Local Commands**: Always listed.
/// 2.  **Remote Commands**: Fetched from the connected application.
///
/// If a remote command has the same name as a local command, the local command
/// takes precedence and the remote one is hidden from the list.
pub struct List;

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
        let mut str = String::new();
        let mut seen_commands = HashSet::new();
        for tool in crate::local_commands::COMMANDS.iter() {
            str.push_str(tool.name());
            str.push_str(": ");
            str.push_str(tool.short_description());
            str.push('\n');
            seen_commands.insert(tool.name());
        }
        //fetch remote commands
        match &*CLIENT {
            Ok(client) => {
                let reply_id = client.next_reply_id();
                let rpc =
                    RPC::Command(CommandInvocation::new("list".to_string(), vec![], reply_id));
                client.send_rpc(rpc).unwrap();
                let msg = client.pop_msg(reply_id);
                match msg {
                    Ok(msg) => {
                        if !msg.success {
                            return Err(Response::String(msg.response.to_string()));
                        }
                        match msg.response {
                            Response::Bytes(bytes) => {
                                match rmp_serde::from_slice::<Vec<ListItem>>(&bytes) {
                                    Err(e) => {
                                        return Err(Response::from(e.to_string()));
                                    }
                                    Ok(items) => {
                                        for item in items {
                                            if seen_commands.contains(item.name.as_str()) {
                                                continue; //skip this command; overridden by a local version
                                            }
                                            str.push_str(&item.name);
                                            str.push_str(": ");
                                            str.push_str(&item.short_description);
                                            str.push('\n');
                                        }
                                    }
                                }
                            }
                            _ => {
                                return Err(Response::String(
                                    "Expected Bytes response from list command".to_string(),
                                ));
                            }
                        }
                    }
                    Err(e) => return Err(e.to_string().into()),
                }
            }
            Err(error) => {
                str.push('\n');
                str.push_str(
                    "Not connected to remote application so remote commands are not available: ",
                );
                str.push_str(&error.to_string());
                str.push('\n');
            }
        }
        Ok(str.into())
    }
}
