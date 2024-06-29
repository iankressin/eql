use crate::common::chain::Chain;
use alloy::{
    eips::BlockNumberOrTag,
    primitives::{Address, Bytes, FixedBytes, B256, U256},
};
use tabled::Tabled;

use std::{error::Error, fmt::Display, str::FromStr};

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

#[derive(Debug, PartialEq, Eq)]
pub enum EntityId {
    Block(BlockNumberOrTag),
    Transaction(FixedBytes<32>),
    Account(Address),
}

// TODO: return instance of Error trait instead of &'static str
impl TryFrom<&str> for EntityId {
    type Error = &'static str;

    fn try_from(id: &str) -> Result<Self, Self::Error> {
        if id.starts_with("0x") {
            if id.len() == 42 {
                let address = Address::from_str(id).map_err(|_| "Invalid address")?;
                Ok(EntityId::Account(address))
            } else if id.len() == 66 {
                let tx_hash = FixedBytes::from_str(id).map_err(|_| "Invalid tx hash")?;
                Ok(EntityId::Transaction(tx_hash))
            } else {
                // Return error: type not supported
                Err("Type not supported")
            }
        } else {
            let block_number = id
                .parse::<u64>()
                .map_err(|_| "Invalid block number")?
                .into();

            Ok(EntityId::Block(block_number))
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum EntityIdError {
    InvalidAddress,
    InvalidTxHash,
    InvalidBlockNumber,
}

impl Display for EntityIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityIdError::InvalidAddress => write!(f, "Invalid address"),
            EntityIdError::InvalidTxHash => write!(f, "Invalid tx hash"),
            EntityIdError::InvalidBlockNumber => write!(f, "3. Invalid block number"),
        }
    }
}

impl Error for EntityIdError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

impl EntityId {
    pub fn to_block_number(&self) -> Result<BlockNumberOrTag, EntityIdError> {
        match self {
            EntityId::Block(block_id) => Ok(*block_id),
            _ => Err(EntityIdError::InvalidBlockNumber),
        }
    }

    pub fn to_tx_hash(&self) -> Result<FixedBytes<32>, EntityIdError> {
        match self {
            EntityId::Transaction(tx_hash) => Ok(*tx_hash),
            _ => Err(EntityIdError::InvalidTxHash),
        }
    }

    pub fn to_address(&self) -> Result<Address, EntityIdError> {
        match self {
            EntityId::Account(address) => Ok(*address),
            _ => Err(EntityIdError::InvalidAddress),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Tabled)]
pub struct BlockQueryRes {
    #[tabled(display_with = "display_option")]
    pub number: Option<u64>,
    #[tabled(display_with = "display_option")]
    pub timestamp: Option<u64>,
    #[tabled(display_with = "display_option")]
    pub hash: Option<B256>,
    #[tabled(display_with = "display_option")]
    pub size: Option<U256>,
    #[tabled(display_with = "display_option")]
    pub parent_hash: Option<B256>,
}

impl Default for BlockQueryRes {
    fn default() -> Self {
        Self {
            number: None,
            timestamp: None,
            hash: None,
            size: None,
            parent_hash: None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Tabled)]
pub struct AccountQueryRes {
    #[tabled(display_with = "display_option")]
    pub nonce: Option<u64>,
    #[tabled(display_with = "display_option")]
    pub balance: Option<U256>,
    #[tabled(display_with = "display_option")]
    pub address: Option<Address>,
}

impl Default for AccountQueryRes {
    fn default() -> Self {
        Self {
            nonce: None,
            balance: None,
            address: None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Tabled)]
pub struct TransactionQueryRes {
    #[tabled(display_with = "display_option")]
    pub hash: Option<FixedBytes<32>>,
    #[tabled(display_with = "display_option")]
    pub from: Option<Address>,
    #[tabled(display_with = "display_option")]
    pub to: Option<Address>,
    #[tabled(display_with = "display_option")]
    pub data: Option<Bytes>,
    #[tabled(display_with = "display_option")]
    pub value: Option<U256>,
    #[tabled(display_with = "display_option")]
    pub gas_price: Option<u128>,
    #[tabled(display_with = "display_option")]
    pub status: Option<bool>,
}

impl Default for TransactionQueryRes {
    fn default() -> Self {
        Self {
            hash: None,
            from: None,
            to: None,
            data: None,
            value: None,
            gas_price: None,
            status: None,
        }
    }
}

// TODO: move to another file
fn display_option<T: std::fmt::Display>(value: &Option<T>) -> String {
    match value {
        Some(value) => value.to_string(),
        None => "-".to_string(),
    }
}
