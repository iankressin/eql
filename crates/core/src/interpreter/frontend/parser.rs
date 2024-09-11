use crate::common::entity_filter::EntityFilter;
use crate::common::entity_id::EntityId;
use crate::common::types::{Expression, Field, GetExpression};
use pest::iterators::Pairs;
use pest::Parser as PestParser;
use pest_derive::Parser as DeriveParser;
use std::error::Error;

#[derive(DeriveParser)]
#[grammar = "src/interpreter/frontend/productions.pest"]
pub struct Parser<'a> {
    source: &'a str,
}

#[derive(Debug, thiserror::Error)]
pub enum ParserError {
    #[error("Unexpected token: {0}")]
    UnexpectedToken(String),
    #[error("Missing entity")]
    MissingEntity,
    #[error("Missing entity_id {0}")]
    PestCustomError(String),
}

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
        // Entity is needed before analyzing the fields so we can determine the type of the fields
        let mut current_pair = pairs;

        while let Some(pair) = current_pair.next() {
            match pair.as_rule() {
                Rule::fields => {
                    let inner_pair = pair.clone().into_inner();
                    get_expr.fields = self.get_fields(inner_pair)?;
                }
                Rule::entity => get_expr.entity = pair.as_str().try_into()?,
                Rule::entity_id => {
                    get_expr.entity_id = Some(pair
                        .into_inner()
                        .map(|pair| pair.try_into())
                        .collect::<Result<Vec<EntityId>, _>>()?);
                }
                Rule::entity_filter => {
                    get_expr.entity_filter = Some(pair
                        .into_inner()
                        .map(|pair| pair.try_into())
                        .collect::<Result<Vec<EntityFilter>, _>>()?);
                } 
                Rule::chain => get_expr.chain = pair.as_str().try_into()?,
                // TODO: the name of the file is being stored along with the operator >
                Rule::dump => get_expr.dump = Some(pair.try_into()?),
                _ => {
                    return Err(Box::new(ParserError::UnexpectedToken(
                        pair.as_str().to_string(),
                    )));
                }
            }
        }

        Ok(get_expr)
    }

    fn get_fields(&self, pairs: Pairs<Rule>) -> Result<Vec<Field>, Box<dyn Error>> {
        let mut fields: Vec<Field> = Vec::new();

        for pair in pairs {
            match pair.as_rule() {
                Rule::account_field => {
                    fields.push(Field::Account(pair.as_str().try_into()?));
                }
                Rule::block_field => {
                    fields.push(Field::Block(pair.as_str().try_into()?));
                }
                Rule::tx_field => {
                    fields.push(Field::Transaction(pair.as_str().try_into()?));
                }
                Rule::log_field => {
                    fields.push(Field::Log(pair.as_str().try_into()?));
                }
                Rule::star_operator => {
                    fields.push(Field::Star);
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
        chain::Chain, ens::NameOrAddress, entity::Entity, entity_filter::BlockRange,
        entity_id::EntityId, types::*,
    };
    use alloy::{
        eips::BlockNumberOrTag,
        primitives::{address, b256, Address},
    };
    use std::str::FromStr;

    #[test]
    fn test_build_ast_with_account_fields() {
        let source =
            "GET nonce, balance, code FROM account 0x1234567890123456789012345678901234567890 ON eth";
        let address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
        let expected = vec![Expression::Get(GetExpression {
            entity: Entity::Account,
            entity_id: Some(vec![EntityId::Account(NameOrAddress::Address(address))]),
            entity_filter: None,
            fields: vec![
                Field::Account(AccountField::Nonce),
                Field::Account(AccountField::Balance),
                Field::Account(AccountField::Code),
            ],
            chain: Chain::Ethereum,
            query: source.to_string(),
            dump: None,
        })];
        let parser = Parser::new(source);

        match parser.parse_expressions() {
            Ok(result) => assert_eq!(result, expected),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[test]
    fn test_build_get_ast_using_ens() {
        let source = "GET nonce, balance FROM account vitalik.eth ON eth";
        let name = String::from("vitalik.eth");
        let expected = vec![Expression::Get(GetExpression {
            entity: Entity::Account,
            entity_id: Some(vec![EntityId::Account(NameOrAddress::Name(name))]),
            entity_filter: None,
            fields: vec![
                Field::Account(AccountField::Nonce),
                Field::Account(AccountField::Balance),
            ],
            chain: Chain::Ethereum,
            query: source.to_string(),
            dump: None,
        })];
        let result = Parser::new(source).parse_expressions().unwrap();

        assert_eq!(result, expected);
    }

    #[test]
    fn test_build_get_ast_with_block_fields() {
        let source = "GET parent_hash, state_root, transactions_root, receipts_root, logs_bloom, extra_data, mix_hash, total_difficulty, base_fee_per_gas, withdrawals_root, blob_gas_used, excess_blob_gas, parent_beacon_block_root, size FROM block 1 ON eth";

        let expected = vec![Expression::Get(GetExpression {
            entity: Entity::Block,
            entity_id: Some(vec![EntityId::Block(BlockRange::new(BlockNumberOrTag::Number(1), None))]),
            entity_filter: None,
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
            dump: None,
        })];

        let parser = Parser::new(source);

        match parser.parse_expressions() {
            Ok(result) => assert_eq!(result, expected),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[test]
    fn test_build_get_ast_using_block_ranges() {
        let source = "GET timestamp FROM block 1:2 ON eth";
        let expected = vec![Expression::Get(GetExpression {
            entity: Entity::Block,
            entity_id: Some(vec![EntityId::Block(BlockRange::new(
                BlockNumberOrTag::Number(1),
                Some(BlockNumberOrTag::Number(2)),
            ))]),
            entity_filter: None,
            fields: vec![Field::Block(BlockField::Timestamp)],
            chain: Chain::Ethereum,
            query: source.to_string(),
            dump: None,
        })];
        let result = Parser::new(source).parse_expressions();

        match result {
            Ok(result) => assert_eq!(result, expected),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[test]
    fn test_build_ast_with_transaction_fields() {
        let source = "GET transaction_type, hash, from, to, data, value, gas_price, gas, status, chain_id, v, r, s, max_fee_per_blob_gas, max_fee_per_gas, max_priority_fee_per_gas, y_parity FROM tx 0x8a6a279a4d28dcc62bcb2f2a3214c93345c107b74f3081754e27471c50783f81 ON eth";

        let expected = vec![Expression::Get(GetExpression {
            entity: Entity::Transaction,
            entity_id: Some(vec![EntityId::Transaction(b256!(
                "8a6a279a4d28dcc62bcb2f2a3214c93345c107b74f3081754e27471c50783f81"
            ))]),
            entity_filter: None,
            fields: vec![
                Field::Transaction(TransactionField::TransactionType),
                Field::Transaction(TransactionField::Hash),
                Field::Transaction(TransactionField::From),
                Field::Transaction(TransactionField::To),
                Field::Transaction(TransactionField::Data),
                Field::Transaction(TransactionField::Value),
                Field::Transaction(TransactionField::GasPrice),
                Field::Transaction(TransactionField::Gas),
                Field::Transaction(TransactionField::Status),
                Field::Transaction(TransactionField::ChainId),
                Field::Transaction(TransactionField::V),
                Field::Transaction(TransactionField::R),
                Field::Transaction(TransactionField::S),
                Field::Transaction(TransactionField::MaxFeePerBlobGas),
                Field::Transaction(TransactionField::MaxFeePerGas),
                Field::Transaction(TransactionField::MaxPriorityFeePerGas),
                Field::Transaction(TransactionField::YParity),
            ],
            chain: Chain::Ethereum,
            query: source.to_string(),
            dump: None,
        })];

        match Parser::new(source).parse_expressions() {
            Ok(result) => assert_eq!(result, expected),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[test]
    fn test_build_ast_with_dump() {
        let source = "GET balance FROM account vitalik.eth ON eth > dump.csv";

        let expected = vec![Expression::Get(GetExpression {
            entity: Entity::Account,
            entity_id: Some(vec![EntityId::Account(NameOrAddress::Name(
                "vitalik.eth".to_string(),
            ))]),
            entity_filter: None,
            fields: vec![Field::Account(AccountField::Balance)],
            chain: Chain::Ethereum,
            query: source.to_string(),
            dump: Some(Dump::new("dump".to_string(), DumpFormat::Csv)),
        })];

        match Parser::new(source).parse_expressions() {
            Ok(result) => assert_eq!(result, expected),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[test]
    fn test_build_ast_with_log_fields() {
        let source = "GET address, topic0, topic1, topic2, topic3, data, block_hash, block_number, block_timestamp, transaction_hash, transaction_index, log_index, removed FROM log WHERE block 4638757, address 0xdAC17F958D2ee523a2206206994597C13D831ec7, topic0 0xcb8241adb0c3fdb35b70c24ce35c5eb0c17af7431c99f827d44a445ca624176a ON eth,
        GET address FROM log WHERE block_hash 0xedb7f4a64744594838f7d9888883ae964fcb4714f6fe5cafb574d3ed6141ad5b, event_signature Transfer(address,address,uint256), topic1 0x00000000000000000000000036928500Bc1dCd7af6a2B4008875CC336b927D57, topic2 0x000000000000000000000000C6CDE7C39eB2f0F0095F41570af89eFC2C1Ea828 ON eth";

        let expected = vec![
        Expression::Get(GetExpression {
            entity: Entity::Log,
            entity_id: None,
            entity_filter: Some(
                vec![
                    EntityFilter::LogBlockRange(BlockRange::new(BlockNumberOrTag::Number(4638757), None)),
                    EntityFilter::LogEmitterAddress(address!("dac17f958d2ee523a2206206994597c13d831ec7")),
                    EntityFilter::LogTopic0(b256!("cb8241adb0c3fdb35b70c24ce35c5eb0c17af7431c99f827d44a445ca624176a")),
                ],
            ),
            fields: vec![
                Field::Log(LogField::Address),
                Field::Log(LogField::Topic0),
                Field::Log(LogField::Topic1),
                Field::Log(LogField::Topic2),
                Field::Log(LogField::Topic3),
                Field::Log(LogField::Data),
                Field::Log(LogField::BlockHash),
                Field::Log(LogField::BlockNumber),
                Field::Log(LogField::BlockTimestamp),
                Field::Log(LogField::TransactionHash),
                Field::Log(LogField::TransactionIndex),
                Field::Log(LogField::LogIndex),
                Field::Log(LogField::Removed),
            ],
            chain: Chain::Ethereum,
            query: "GET address, topic0, topic1, topic2, topic3, data, block_hash, block_number, block_timestamp, transaction_hash, transaction_index, log_index, removed FROM log WHERE block 4638757, address 0xdAC17F958D2ee523a2206206994597C13D831ec7, topic0 0xcb8241adb0c3fdb35b70c24ce35c5eb0c17af7431c99f827d44a445ca624176a ON eth,\n        ".to_string(),
            dump: None,
        }),

        Expression::Get(GetExpression {
            entity: Entity::Log,
            entity_id: None,
            entity_filter: Some(
                vec![
                    EntityFilter::LogBlockHash(b256!("edb7f4a64744594838f7d9888883ae964fcb4714f6fe5cafb574d3ed6141ad5b")),
                    EntityFilter::LogEventSignature(String::from("Transfer(address,address,uint256)")),
                    EntityFilter::LogTopic1(b256!("00000000000000000000000036928500bc1dcd7af6a2b4008875cc336b927d57")),
                    EntityFilter::LogTopic2(b256!("000000000000000000000000c6cde7c39eb2f0f0095f41570af89efc2c1ea828")),
                ],
            ),
            fields: vec![
                Field::Log(LogField::Address),
            ],
            chain: Chain::Ethereum,
            query: "GET address FROM log WHERE block_hash 0xedb7f4a64744594838f7d9888883ae964fcb4714f6fe5cafb574d3ed6141ad5b, event_signature Transfer(address,address,uint256), topic1 0x00000000000000000000000036928500Bc1dCd7af6a2B4008875CC336b927D57, topic2 0x000000000000000000000000C6CDE7C39eB2f0F0095F41570af89eFC2C1Ea828 ON eth".to_string(),
            dump: None,
        }),
        ];

        match Parser::new(source).parse_expressions() {
            Ok(result) => assert_eq!(result, expected),
            Err(e) => panic!("Error: {}", e),
        }
    }
}
