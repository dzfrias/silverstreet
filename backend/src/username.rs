use std::ops::Deref;

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct Username(SmolStr);

impl Default for Username {
    fn default() -> Self {
        Self::new("NULL USERNAME")
    }
}

impl Username {
    pub fn new(s: &str) -> Self {
        Self(s.into())
    }
}

impl From<&str> for Username {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for Username {
    fn from(value: String) -> Self {
        Self(value.into())
    }
}

impl Deref for Username {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
