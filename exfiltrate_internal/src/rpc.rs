use crate::command::Response;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub enum RPC {
    Command(CommandInvocation),
    CommandResponse(CommandResponse),
}

#[derive(Serialize, Deserialize)]
pub struct CommandInvocation {
    pub name: String,
    pub args: Vec<String>,
    ///Responses must include this field to correlate with requests
    pub reply_id: u32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CommandResponse {
    pub success: bool,
    pub response: Response,
    pub reply_id: u32,
    pub num_attachments: u32,
}

impl CommandResponse {
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
    pub fn new(name: String, args: Vec<String>, reply_id: u32) -> Self {
        CommandInvocation {
            name,
            args,
            reply_id,
        }
    }
}
