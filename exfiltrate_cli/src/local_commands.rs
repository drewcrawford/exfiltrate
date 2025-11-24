// SPDX-License-Identifier: MIT OR Apache-2.0
//! Local commands are executed directly by the CLI tool, as opposed to remote commands
//! which are executed by the target application.
//!
//! # Local vs Remote Commands
//!
//! *   **Local Commands**: Built into the `exfiltrate` binary (e.g., `help`, `list`, `status`).
//!     They are always available, even if the target application is not running.
//! *   **Remote Commands**: Defined in the target application and accessed via RPC.
//!     They are only available when the CLI is connected to the application.
//!
//! # Interaction
//!
//! Some local commands, like `help` and `list`, interact with the remote application
//! to provide a unified experience. For example:
//! *   `list` shows both local and remote commands.
//! *   `help` falls back to querying the remote application if a local topic is not found.

use exfiltrate_internal::command::Command;
use std::sync::LazyLock;
mod help;
pub(crate) mod list;
mod status;

/// A registry of all available local commands.
///
/// These commands are instantiated lazily and are always available to the CLI.
pub(crate) static COMMANDS: LazyLock<Vec<Box<dyn Command>>> = LazyLock::new(|| {
    vec![
        Box::new(list::List),
        Box::new(status::Status),
        Box::new(help::Help),
    ]
});
