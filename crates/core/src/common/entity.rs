use std::fmt::Display;

use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum Entity {
    Block,
    Transaction,
    Account,
    Log,
}

impl Display for Entity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Entity::Block => write!(f, "block"),
            Entity::Transaction => write!(f, "transaction"),
            Entity::Account => write!(f, "account"),
            Entity::Log => write!(f, "log"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EntityError {
    #[error("Invalid entity: {0}")]
    InvalidEntity(String),
}

impl TryFrom<&str> for Entity {
    type Error = EntityError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "block" => Ok(Entity::Block),
            "tx" => Ok(Entity::Transaction),
            "account" => Ok(Entity::Account),
            "log" => Ok(Entity::Log),
            invalid_entity => Err(EntityError::InvalidEntity(invalid_entity.to_string())),
        }
    }
}

impl TryFrom<String> for Entity {
    type Error = EntityError;

    fn try_from(entity: String) -> Result<Self, Self::Error> {
        Entity::try_from(entity.as_str())
    }
}
