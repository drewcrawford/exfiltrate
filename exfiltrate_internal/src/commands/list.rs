use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ListItem {
    pub name: String,
    pub short_description: String,
}
