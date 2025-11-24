// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::help::HelpTopic;

/// Help topic for writing custom commands.
///
/// This topic provides a guide on how to extend exfiltrate with user-defined
/// commands, including syntax, response formats, and registration.
pub const CUSTOM_COMMANDS: HelpTopic = HelpTopic {
    name: "custom_commands",
    short: "Provides information about writing custom commands.  Use this topic to understand how to create custom commands.",
    full: include_str!("custom_commands.md"),
};
