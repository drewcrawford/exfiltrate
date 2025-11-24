use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::hash::Hash;

/// An item in the command list, representing a single available command.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ListItem {
    /// The name of the command.
    pub name: String,
    /// A short description of what the command does.
    pub short_description: String,
}

impl Display for ListItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.name, self.short_description)
    }
}
