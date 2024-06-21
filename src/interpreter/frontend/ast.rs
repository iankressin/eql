use alloy::primitives::{Address, FixedBytes};

#[derive(Debug)]
pub enum AstNode {
    Verb,
    Fields(Vec<Field>),
    Transaction(FixedBytes<32>),
    Block(u64),
    Account(Address),
    Chain,
}

#[derive(Debug)]
pub enum Verb {
    Get,
    Send,
    Sum,
    Times,
    Div,
}

#[derive(Debug)]
pub enum Field {
    Address,
    Nonce,
    Balance,
    Number,
    Timestamp,
    Size,
    Hash,
    ParentHash,
    Reward,
    From,
    To,
    Data,
    Value,
    Fee,
    GasPrice,
    Status,
}

impl TryFrom<&str> for Field {
    type Error = &'static str;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "address" => Ok(Field::Address),
            "nonce" => Ok(Field::Nonce),
            "balance" => Ok(Field::Balance),
            "number" => Ok(Field::Number),
            "timestamp" => Ok(Field::Timestamp),
            "size" => Ok(Field::Size),
            "hash" => Ok(Field::Hash),
            "parent_hash" => Ok(Field::ParentHash),
            "reward" => Ok(Field::Reward),
            "from" => Ok(Field::From),
            "to" => Ok(Field::To),
            "data" => Ok(Field::Data),
            "value" => Ok(Field::Value),
            "fee" => Ok(Field::Fee),
            "gas_price" => Ok(Field::GasPrice),
            "status" => Ok(Field::Status),
            invalid_field => {
                println!("Invalid field {}", invalid_field);
                Err("Invalid field {}")
            },
        }
    }
}
