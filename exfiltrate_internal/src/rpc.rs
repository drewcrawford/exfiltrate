// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::command::Response;
use serde::{Deserialize, Serialize};
use std::fmt::Display;

/// Remote procedure call message types for communication between client and server.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RPC {
    /// A command invocation request from the client.
    Command(CommandInvocation),
    /// A response to a command invocation.
    CommandResponse(CommandResponse),
}

/// A request to invoke a command on the server.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommandInvocation {
    /// The name of the command to invoke.
    pub name: String,
    /// Arguments to pass to the command.
    pub args: Vec<String>,
    /// Request identifier for correlating responses.
    pub reply_id: u32,
}

/// A response to a command invocation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommandResponse {
    /// Whether the command succeeded.
    pub success: bool,
    /// The command's output or error message.
    pub response: Response,
    /// The request identifier this response corresponds to.
    pub reply_id: u32,
    /// The number of binary attachments that follow this response.
    pub num_attachments: u32,
}

impl CommandResponse {
    /// Creates a new command response.
    pub fn new(success: bool, response: Response, reply_id: u32) -> Self {
        CommandResponse {
            success,
            response,
            reply_id,
            num_attachments: 0,
        }
    }
}

impl CommandInvocation {
    /// Creates a new command invocation request.
    pub fn new(name: String, args: Vec<String>, reply_id: u32) -> Self {
        CommandInvocation {
            name,
            args,
            reply_id,
        }
    }
}

impl Display for RPC {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RPC::Command(cmd) => write!(f, "Command({})", cmd),
            RPC::CommandResponse(resp) => write!(f, "CommandResponse({})", resp),
        }
    }
}

impl Display for CommandInvocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.args.is_empty() {
            write!(f, "{} [id={}]", self.name, self.reply_id)
        } else {
            write!(f, "{} {:?} [id={}]", self.name, self.args, self.reply_id)
        }
    }
}

impl Display for CommandResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = if self.success { "ok" } else { "err" };
        write!(f, "[id={}] {}: {}", self.reply_id, status, self.response)
    }
}
