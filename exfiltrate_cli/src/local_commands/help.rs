// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::wire::client::CLIENT;
use exfiltrate_internal::command::{Command, Response};
use exfiltrate_internal::rpc::{CommandInvocation, RPC};

/// The local `help` command.
///
/// This command resolves help requests in the following order:
/// 1.  Checks for a static help topic (e.g., `integration`).
/// 2.  Checks for a local command (e.g., `list`).
/// 3.  Attempts to query the remote application for help on a remote command.
pub struct Help;

impl Command for Help {
    fn name(&self) -> &'static str {
        "help"
    }

    fn short_description(&self) -> &'static str {
        "Provides detailed help for a command or help topic.  Use this to learn how to use a command or display a help topic."
    }

    fn full_description(&self) -> &'static str {
        "Provides detailed help for a command or help topic.
Usage: exfiltrate help [COMMAND] (or topic)"
    }

    fn execute(&self, args: Vec<String>) -> Result<Response, Response> {
        if args.is_empty() {
            return Err("No command provided".into());
        }
        //check if it's a non-command help topic
        if let Some(topic) = crate::help::help_topic(&args[0]) {
            return Ok(topic.into());
        }
        let requested = &args[0];
        for command in super::COMMANDS.iter() {
            if command.name() == requested {
                return Ok(command.full_description().into());
            }
        }
        //try remote commands
        match CLIENT.as_ref() {
            Err(e) => Err(format!("Can't connect to application: {}", e).into()),
            Ok(client) => {
                let reply_id = client.next_reply_id();
                let command = CommandInvocation::new("help".to_string(), args, reply_id);
                let send_op = client.send_rpc(RPC::Command(command));
                match send_op {
                    Err(e) => Err(e.to_string().into()),
                    Ok(_) => {
                        let reply = client.pop_msg(reply_id);
                        match reply {
                            Err(e) => Err(e.to_string().into()),
                            Ok(response) => Ok(response.response),
                        }
                    }
                }
            }
        }
    }
}
