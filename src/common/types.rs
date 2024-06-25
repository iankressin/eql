use std::str::FromStr;
use alloy::primitives::{Address, FixedBytes};
use crate::common::chain::Chain;

#[derive(Debug, PartialEq, Eq)]
pub enum Expression {
    Get(GetExpression)
}

#[derive(Debug, PartialEq, Eq)]
pub struct GetExpression {
    pub entity: Entity,
    pub entity_id: EntityId,
    pub fields: Vec<Field>,
    pub chain: Chain,
}

impl Default for GetExpression {
    fn default() -> Self {
        Self {
            entity: Entity::Block,
            entity_id: EntityId::Block(0),
            fields: vec![],
            chain: Chain::Ethereum,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Field {
    Account(AccountField),
    Block(BlockField),
    Transaction(TransactionField),
}

impl TryFrom<&str> for Field {
    type Error = &'static str;

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
            "reward" => Ok(Field::Block(BlockField::Reward)),
            "from" => Ok(Field::Transaction(TransactionField::From)),
            "to" => Ok(Field::Transaction(TransactionField::To)),
            "data" => Ok(Field::Transaction(TransactionField::Data)),
            "value" => Ok(Field::Transaction(TransactionField::Value)),
            "fee" => Ok(Field::Transaction(TransactionField::Fee)),
            "gas_price" => Ok(Field::Transaction(TransactionField::GasPrice)),
            "status" => Ok(Field::Transaction(TransactionField::Status)),
            _ => Err("Invalid field"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum AccountField {
    Address,
    Nonce,
    Balance,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BlockField {
    Number,
    Timestamp,
    Size,
    Hash,
    ParentHash,
    Reward,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TransactionField {
    Hash,
    From,
    To,
    Data,
    Value,
    Fee,
    GasPrice,
    Status,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Entity {
    Block,
    Transaction,
    Account,
}

impl TryFrom<&str> for Entity {
    type Error = &'static str;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "block" => Ok(Entity::Block),
            "transaction" => Ok(Entity::Transaction),
            "account" => Ok(Entity::Account),
            invalid_entity => {
                println!("Invalid entity {}", invalid_entity);
                Err("Invalid entity {}")
            },
        }
    }
    
}

#[derive(Debug, PartialEq, Eq)]
pub enum EntityId {
    Block(u64),
    Transaction(FixedBytes<32>),
    Account(Address),
}

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
            let block_number = id.parse::<u64>().map_err(|_| "Invalid block number")?;
            Ok(EntityId::Block(block_number))
        }
    }
}
