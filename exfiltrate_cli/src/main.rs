//! The main entry point for the `exfiltrate` CLI tool.
//!
//! This binary acts as a client that connects to a running application
//! (which must be using the `exfiltrate` library) to execute debugging commands.
//!
//! # Architecture
//!
//! 1.  **Argument Parsing**: Reads command-line arguments to determine the command to run.
//! 2.  **Local Dispatch**: Checks if the command is a built-in local command (like `help` or `list`).
//! 3.  **Remote Dispatch**: If not local, connects to the remote application via TCP and sends an RPC.
//! 4.  **Response Handling**: Receives the response (text, files, or images) and handles it (printing or saving to disk).

use exfiltrate_internal::command::{Command, Response};
use exfiltrate_internal::rpc::{CommandInvocation, RPC};
use local_commands::list::List;
use rand::Rng;
use rand::distr::Alphanumeric;
use std::io::Write;
use std::path::PathBuf;
use webp::PixelLayout;
use wire::client::CLIENT;

mod help;
mod local_commands;
mod wire;

/// Entry point for the CLI.
///
/// Parses arguments and delegates to `dispatch` or `help`.
fn main() {
    let exe_args = std::env::args().collect::<Vec<String>>();
    let args = exe_args[1..].to_vec();
    if args.is_empty()
        || args[0] == "-h"
        || args[0] == "--help"
        || (args[0] == "help" && args.len() == 1)
    {
        help();
        return;
    }
    //try to dispatch a command
    match dispatch(args) {
        Ok(result) => {
            println!("{}", result);
        }
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}

/// Dispatches a command to either a local handler or the remote application.
///
/// 1.  Checks `local_commands::COMMANDS` for a match.
/// 2.  If no local match, attempts to connect to the remote application via `CLIENT`.
/// 3.  Sends the command via RPC and waits for a response.
/// 4.  Handles the response (saving files/images or returning text).
fn dispatch(args: Vec<String>) -> Result<String, String> {
    //first, try local commands
    let command_name = args[0].to_string();
    let forwarded_args = args[1..].to_vec();
    for command in local_commands::COMMANDS.iter() {
        if command.name() == args[0] {
            let r = command.execute(forwarded_args);
            match r {
                Ok(result) => {
                    return Ok(result.to_string());
                }
                Err(e) => {
                    return Err(e.to_string());
                }
            }
        }
    }
    //now try remote commands
    match CLIENT.as_ref() {
        Err(e) => Err(e.to_string()),
        Ok(client) => {
            let reply_id = client.next_reply_id();
            let command_invocation = CommandInvocation::new(command_name, forwarded_args, reply_id);
            let r = client.send_rpc(RPC::Command(command_invocation));
            match r {
                Err(e) => Err(e.to_string()),

                Ok(_) => {
                    let reply = client.pop_msg(reply_id);
                    match reply {
                        Err(e) => Err(e.to_string()),

                        Ok(r) => {
                            if !r.success {
                                Err(r.response.to_string())
                            } else {
                                match r.response {
                                    Response::String(s) => Ok(s),
                                    Response::Files(files) => {
                                        let mut output = String::new();
                                        for f in files {
                                            if let Some(remark) = &f.remark {
                                                output.push_str(remark);
                                                output.push('\n');
                                            }
                                            // create a random filename
                                            let rand_string: String = rand::rng()
                                                .sample_iter(&Alphanumeric)
                                                .take(5)
                                                .map(char::from)
                                                .collect();
                                            let mut path = PathBuf::from(".");
                                            path.push(format!(
                                                "{}.{}",
                                                rand_string,
                                                f.proposed_extension.trim_start_matches('.')
                                            ));
                                            let mut file = std::fs::File::create(&path).unwrap();
                                            let write_result = file.write_all(&f.contents);
                                            match write_result {
                                                Ok(..) => {}
                                                Err(e) => {
                                                    return Err(e.to_string());
                                                }
                                            }
                                            output.push_str(&format!(
                                                "Wrote {bytes} bytes to {path}\n",
                                                bytes = f.contents.len(),
                                                path = path.to_str().unwrap()
                                            ));
                                        }
                                        Ok(output)
                                    }
                                    Response::Images(images) => {
                                        let mut output = String::new();
                                        for info in images {
                                            if let Some(remark) = &info.remark {
                                                output.push_str(remark);
                                                output.push('\n');
                                            }
                                            // create a random filename
                                            let rand_string: String = rand::rng()
                                                .sample_iter(&Alphanumeric)
                                                .take(5)
                                                .map(char::from)
                                                .collect();
                                            let mut path = PathBuf::from(".");
                                            path.push(format!("{}.{}", rand_string, "webp"));
                                            let mut file = std::fs::File::create(&path).unwrap();
                                            let data: &[u8] = bytemuck::cast_slice(&info.data);
                                            let time = std::time::Instant::now();
                                            let encode = webp::Encoder::new(
                                                data,
                                                PixelLayout::Rgba,
                                                info.width,
                                                info.height,
                                            );
                                            let r = encode.encode_lossless();
                                            eprintln!(
                                                "Encoded in {} ms to {} bytes",
                                                time.elapsed().as_millis(),
                                                r.len()
                                            );
                                            file.write_all(&r).unwrap();
                                            output.push_str("Wrote image to ");
                                            output.push_str(path.as_os_str().to_str().unwrap());
                                            output.push('\n');
                                        }
                                        Ok(output)
                                    }
                                    _ => {
                                        todo!()
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn list() {
    let list = List::execute(&List, vec![]).unwrap();
    eprintln!("Commands:");
    eprintln!("{}", list);
}

fn help() {
    eprintln!("A tool to debug running Rust programs");
    eprintln!();
    eprintln!("Usage: exfiltrate [COMMAND] [OPTIONS]");
    eprintln!("Help: exfiltrate help");
    eprintln!("Command-specific help: exfiltrate help [COMMAND]");
    eprintln!();
    list();

    eprintln!("Additional help topics: (use exfiltrate [TOPIC] to display the topic)");
    help::brief_help();
}
