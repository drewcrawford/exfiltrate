// SPDX-License-Identifier: MIT OR Apache-2.0
//! The help module provides a system for displaying documentation and guides
//! within the exfiltrate CLI.
//!
//! It manages a collection of [`HelpTopic`]s, which are static descriptions
//! of various features or workflows (like integration guides or custom commands).
//! These topics can be listed briefly or displayed in full detail.

mod custom_commands;
mod integration;

/// Represents a single help topic with a name, short summary, and full description.
pub struct HelpTopic {
    /// The unique name of the topic, used to request it via `exfiltrate help <name>`.
    pub name: &'static str,
    /// A one-line summary of what the topic covers.
    pub short: &'static str,
    /// The complete documentation for the topic, usually included from a markdown file.
    pub full: &'static str,
}

/// The registry of all available help topics.
const HELP_TOPICS: &[HelpTopic] = &[integration::INTEGRATION, custom_commands::CUSTOM_COMMANDS];

/// Prints a list of all available help topics and their short descriptions to stderr.
pub fn brief_help() {
    for topic in HELP_TOPICS {
        eprintln!("{}: {}", topic.name, topic.short);
    }
}

/// Retrieves the full help text for a specific topic, if it exists.
///
/// Returns `Some(String)` containing the formatted help text if the topic is found,
/// or `None` if no topic matches the requested name.
pub fn help_topic(requested_topic: &str) -> Option<String> {
    for topic in HELP_TOPICS {
        if topic.name == requested_topic {
            let mut str = String::new();
            str.push_str(topic.name);
            str.push_str(" help:\n");
            str.push_str(topic.full);
            return Some(str);
        }
    }
    None
}
