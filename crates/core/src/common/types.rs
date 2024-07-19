use super::{chain::Chain, entity_id::EntityId};
use alloy::eips::BlockNumberOrTag;
use std::{error::Error, fmt::Display};

#[derive(Debug, PartialEq, Eq)]
pub enum Expression {
    Get(GetExpression),
}

#[derive(Debug, PartialEq, Eq)]
pub struct GetExpression {
    pub entity: Entity,
    pub entity_id: EntityId,
    pub fields: Vec<Field>,
    pub chain: Chain,
    pub query: String,
}

impl Default for GetExpression {
    fn default() -> Self {
        Self {
            entity: Entity::Block,
            entity_id: EntityId::Block(BlockNumberOrTag::Earliest),
            fields: vec![],
            chain: Chain::Ethereum,
            query: "".to_string(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Field {
    Account(AccountField),
    Block(BlockField),
    Transaction(TransactionField),
}

impl Display for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Field::Account(account_field) => write!(f, "{}", account_field),
            Field::Block(block_field) => write!(f, "{}", block_field),
            Field::Transaction(transaction_field) => write!(f, "{}", transaction_field),
        }
    }
}

#[derive(Debug)]
pub enum FieldError {
    InvalidField(String),
}

impl Display for FieldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FieldError::InvalidField(field) => write!(f, "Invalid field: {}", field),
        }
    }
}

impl Error for FieldError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

impl TryFrom<&str> for Field {
    type Error = Box<dyn Error>;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "address" => Ok(Field::Account(AccountField::Address)),
            "nonce" => Ok(Field::Account(AccountField::Nonce)),
            "balance" => Ok(Field::Account(AccountField::Balance)),
            "number" => Ok(Field::Block(BlockField::Number)),
            "timestamp" => Ok(Field::Block(BlockField::Timestamp)),
            "size" => Ok(Field::Block(BlockField::Size)),
            "hash" => Ok(Field::Block(BlockField::Hash)),
            "parent_hash" => Ok(Field::Block(BlockField::ParentHash)),
            "from" => Ok(Field::Transaction(TransactionField::From)),
            "to" => Ok(Field::Transaction(TransactionField::To)),
            "data" => Ok(Field::Transaction(TransactionField::Data)),
            "value" => Ok(Field::Transaction(TransactionField::Value)),
            "gas_price" => Ok(Field::Transaction(TransactionField::GasPrice)),
            "status" => Ok(Field::Transaction(TransactionField::Status)),
            invalid_field => Err(Box::new(FieldError::InvalidField(
                invalid_field.to_string(),
            ))),
        }
    }
}

impl TryFrom<&Field> for AccountField {
    type Error = Box<dyn Error>;

    fn try_from(field: &Field) -> Result<Self, Self::Error> {
        match field {
            Field::Account(account_field) => Ok(*account_field),
            invalid_field => Err(Box::new(FieldError::InvalidField(format!(
                "Invalid field {:?} for entity account",
                invalid_field
            )))),
        }
    }
}

impl TryFrom<&Field> for BlockField {
    type Error = Box<dyn Error>;

    fn try_from(field: &Field) -> Result<Self, Self::Error> {
        match field {
            Field::Block(block_field) => Ok(*block_field),
            invalid_field => Err(Box::new(FieldError::InvalidField(format!(
                "Invalid field {:?} for entity block",
                invalid_field
            )))),
        }
    }
}

impl TryFrom<&Field> for TransactionField {
    type Error = Box<dyn Error>;

    fn try_from(field: &Field) -> Result<Self, Self::Error> {
        match field {
            Field::Transaction(transaction_field) => Ok(*transaction_field),
            invalid_field => Err(Box::new(FieldError::InvalidField(format!(
                "Invalid field {:?} for entity transaction",
                invalid_field
            )))),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AccountField {
    Address,
    Nonce,
    Balance,
}

impl Display for AccountField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AccountField::Address => write!(f, "address"),
            AccountField::Nonce => write!(f, "nonce"),
            AccountField::Balance => write!(f, "balance"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum BlockField {
    Number,
    Timestamp,
    Size,
    Hash,
    ParentHash,
}

impl Display for BlockField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockField::Number => write!(f, "number"),
            BlockField::Timestamp => write!(f, "timestamp"),
            BlockField::Size => write!(f, "size"),
            BlockField::Hash => write!(f, "hash"),
            BlockField::ParentHash => write!(f, "parent_hash"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum TransactionField {
    Hash,
    From,
    To,
    Data,
    Value,
    GasPrice,
    Status,
}

impl Display for TransactionField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransactionField::Hash => write!(f, "hash"),
            TransactionField::From => write!(f, "from"),
            TransactionField::To => write!(f, "to"),
            TransactionField::Data => write!(f, "data"),
            TransactionField::Value => write!(f, "value"),
            TransactionField::GasPrice => write!(f, "gas_price"),
            TransactionField::Status => write!(f, "status"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Entity {
    Block,
    Transaction,
    Account,
}

impl Display for Entity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Entity::Block => write!(f, "block"),
            Entity::Transaction => write!(f, "transaction"),
            Entity::Account => write!(f, "account"),
        }
    }
}

#[derive(Debug)]
pub enum EntityError {
    InvalidEntity(String),
}

impl Display for EntityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityError::InvalidEntity(entity) => write!(f, "Invalid entity: {}", entity),
        }
    }
}

impl Error for EntityError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

impl TryFrom<&str> for Entity {
    type Error = Box<dyn Error>;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "block" => Ok(Entity::Block),
            "tx" => Ok(Entity::Transaction),
            "account" => Ok(Entity::Account),
            invalid_entity => Err(Box::new(EntityError::InvalidEntity(
                invalid_entity.to_string(),
            ))),
        }
    }
}
