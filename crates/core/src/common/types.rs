use super::{chain::Chain, entity::Entity, entity_id::EntityId};
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

    // TODO: check if we're talking about nonce from the account or the block
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
            "state_root" => Ok(Field::Block(BlockField::StateRoot)),
            "transactions_root" => Ok(Field::Block(BlockField::TransactionsRoot)),
            "receipts_root" => Ok(Field::Block(BlockField::ReceiptsRoot)),
            "logs_bloom" => Ok(Field::Block(BlockField::LogsBloom)),
            "extra_data" => Ok(Field::Block(BlockField::ExtraData)),
            "mix_hash" => Ok(Field::Block(BlockField::MixHash)),
            "total_difficulty" => Ok(Field::Block(BlockField::TotalDifficulty)),
            "base_fee_per_gas" => Ok(Field::Block(BlockField::BaseFeePerGas)),
            "withdrawals_root" => Ok(Field::Block(BlockField::WithdrawalsRoot)),
            "blob_gas_used" => Ok(Field::Block(BlockField::BlobGasUsed)),
            "excess_blob_gas" => Ok(Field::Block(BlockField::ExcessBlobGas)),
            "parent_beacon_block_root" => Ok(Field::Block(BlockField::ParentBeaconBlockRoot)),
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

// TODO: should include nonce, transactions and withdrawals
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum BlockField {
    Number,
    Timestamp,
    Size,
    Hash,
    ParentHash,
    StateRoot,
    TransactionsRoot,
    ReceiptsRoot,
    LogsBloom,
    ExtraData,
    MixHash,
    TotalDifficulty,
    BaseFeePerGas,
    WithdrawalsRoot,
    BlobGasUsed,
    ExcessBlobGas,
    ParentBeaconBlockRoot,
}

impl Display for BlockField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockField::Number => write!(f, "number"),
            BlockField::Timestamp => write!(f, "timestamp"),
            BlockField::Size => write!(f, "size"),
            BlockField::Hash => write!(f, "hash"),
            BlockField::ParentHash => write!(f, "parent_hash"),
            BlockField::StateRoot => write!(f, "state_root"),
            BlockField::TransactionsRoot => write!(f, "transactions_root"),
            BlockField::ReceiptsRoot => write!(f, "receipts_root"),
            BlockField::LogsBloom => write!(f, "logs_bloom"),
            BlockField::ExtraData => write!(f, "extra_data"),
            BlockField::MixHash => write!(f, "mix_hash"),
            BlockField::TotalDifficulty => write!(f, "total_difficulty"),
            BlockField::BaseFeePerGas => write!(f, "base_fee_per_gas"),
            BlockField::WithdrawalsRoot => write!(f, "withdrawals_root"),
            BlockField::BlobGasUsed => write!(f, "blob_gas_used"),
            BlockField::ExcessBlobGas => write!(f, "excess_blob_gas"),
            BlockField::ParentBeaconBlockRoot => write!(f, "parent_beacon_block_root"),
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
