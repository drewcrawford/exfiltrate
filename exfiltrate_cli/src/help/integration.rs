// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::help::HelpTopic;

/// Help topic for integrating exfiltrate into a project.
///
/// This topic covers adding dependencies, feature flags, initialization,
/// and basic usage of the exfiltrate framework.
pub const INTEGRATION: HelpTopic = HelpTopic {
    name: "integration",
    short: "Use this to learn how to integrate exfiltrate into a new project.",
    full: include_str!("integration.md"),
};
