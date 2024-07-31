use std::error::Error;
use std::fmt::Display;

use crate::common::types::{Expression, Field, GetExpression};
use pest::iterators::Pairs;
use pest::Parser as PestParser;
use pest_derive::Parser as DeriveParser;

#[derive(DeriveParser)]
#[grammar = "src/interpreter/frontend/productions.pest"]
pub struct Parser<'a> {
    source: &'a str,
}

#[derive(Debug)]
pub enum ParserError {
    UnexpectedToken(String),
}

impl Display for ParserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParserError::UnexpectedToken(token) => write!(f, "Unexpected token: {}", token),
        }
    }
}

impl std::error::Error for ParserError {}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Self {
        Parser { source }
    }

    pub fn parse_expressions(&self) -> Result<Vec<Expression>, Box<dyn Error>> {
        let mut expressions: Vec<Expression> = vec![];
        let pairs = Parser::parse(Rule::program, self.source)?;

        for pair in pairs {
            match pair.as_rule() {
                Rule::get => {
                    let inner_pair = pair.clone().into_inner();
                    let mut get_expr = self.parse_get_expr(inner_pair)?;
                    // This is being done here since [`inner_pair`] doesn't have the verb. E.g. `GET`
                    get_expr.query = pair.as_str().to_string();
                    expressions.push(Expression::Get(get_expr));
                }
                _ => {
                    return Err(Box::new(ParserError::UnexpectedToken(
                        pair.as_str().to_string(),
                    )))
                }
            }
        }

        Ok(expressions)
    }

    fn parse_get_expr(&self, pairs: Pairs<Rule>) -> Result<GetExpression, Box<dyn Error>> {
        let mut get_expr = GetExpression::default();
        let mut current_pair = pairs;

        while let Some(pair) = current_pair.next() {
            match pair.as_rule() {
                Rule::fields => {
                    let inner_pair = pair.clone().into_inner();
                    get_expr.fields = self.get_fields(inner_pair)?;
                }
                Rule::entity => get_expr.entity = pair.as_str().try_into()?,
                // We shouldn't need to call `trim()` here, but the parser is
                // adding an extra whitespace when entity_id is block number
                Rule::entity_id => get_expr.entity_id = pair.as_str().trim().try_into()?,
                Rule::chain => get_expr.chain = pair.as_str().try_into()?,
                _ => {
                    return Err(Box::new(ParserError::UnexpectedToken(
                        pair.as_str().to_string(),
                    )))
                }
            }
        }

        Ok(get_expr)
    }

    fn get_fields(&self, pairs: Pairs<Rule>) -> Result<Vec<Field>, Box<dyn Error>> {
        let mut fields: Vec<Field> = Vec::new();

        for pair in pairs {
            match pair.as_rule() {
                Rule::account_field | Rule::block_field | Rule::tx_field => {
                    fields.push(pair.as_str().try_into()?);
                }
                _ => {
                    return Err(Box::new(ParserError::UnexpectedToken(
                        pair.as_str().to_string(),
                    )))
                }
            }
        }

        Ok(fields)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::{
        chain::Chain, ens::NameOrAddress, entity::Entity, entity_id::EntityId, types::*,
    };
    use alloy::{eips::BlockNumberOrTag, primitives::Address};
    use std::str::FromStr;

    #[test]
    fn test_build_get_ast() {
        let source =
            "GET nonce, balance FROM account 0x1234567890123456789012345678901234567890 ON eth";
        let address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
        let expected = vec![Expression::Get(GetExpression {
            entity: Entity::Account,
            entity_id: EntityId::Account(NameOrAddress::Address(address)),
            fields: vec![
                Field::Account(AccountField::Nonce),
                Field::Account(AccountField::Balance),
            ],
            chain: Chain::Ethereum,
            query: source.to_string(),
        })];
        let parser = Parser::new(source);
        let result = parser.parse_expressions().unwrap();

        assert_eq!(result, expected);
    }

    #[test]
    fn test_build_get_ast_using_ens() {
        let source = "GET nonce, balance FROM account vitalik.eth ON eth";
        let name = String::from("vitalik.eth");
        let expected = vec![Expression::Get(GetExpression {
            entity: Entity::Account,
            entity_id: EntityId::Account(NameOrAddress::Name(name)),
            fields: vec![
                Field::Account(AccountField::Nonce),
                Field::Account(AccountField::Balance),
            ],
            chain: Chain::Ethereum,
            query: source.to_string(),
        })];
        let parser = Parser::new(source);
        let result = parser.parse_expressions().unwrap();

        assert_eq!(result, expected);
    }

    #[test]
    fn test_build_get_ast_with_block_fields() {
        let source = "GET parent_hash, state_root, transactions_root, receipts_root, logs_bloom, extra_data, mix_hash, total_difficulty, base_fee_per_gas, withdrawals_root, blob_gas_used, excess_blob_gas, parent_beacon_block_root, size FROM block 1 ON eth";

        let expected = vec![Expression::Get(GetExpression {
            entity: Entity::Block,
            entity_id: EntityId::Block(BlockNumberOrTag::Number(1)),
            fields: vec![
                Field::Block(BlockField::ParentHash),
                Field::Block(BlockField::StateRoot),
                Field::Block(BlockField::TransactionsRoot),
                Field::Block(BlockField::ReceiptsRoot),
                Field::Block(BlockField::LogsBloom),
                Field::Block(BlockField::ExtraData),
                Field::Block(BlockField::MixHash),
                Field::Block(BlockField::TotalDifficulty),
                Field::Block(BlockField::BaseFeePerGas),
                Field::Block(BlockField::WithdrawalsRoot),
                Field::Block(BlockField::BlobGasUsed),
                Field::Block(BlockField::ExcessBlobGas),
                Field::Block(BlockField::ParentBeaconBlockRoot),
                Field::Block(BlockField::Size),
            ],
            chain: Chain::Ethereum,
            query: source.to_string(),
        })];

        let parser = Parser::new(source);

        match parser.parse_expressions() {
            Ok(result) => assert_eq!(result, expected),
            Err(e) => panic!("Error: {}", e),
        }
    }
}
