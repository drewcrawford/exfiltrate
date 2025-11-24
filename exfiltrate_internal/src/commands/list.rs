use serde::{Deserialize, Serialize};

/// An item in the command list, representing a single available command.
#[derive(Serialize, Deserialize)]
pub struct ListItem {
    /// The name of the command.
    pub name: String,
    /// A short description of what the command does.
    pub short_description: String,
}
