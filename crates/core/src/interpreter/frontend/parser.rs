use crate::common::types::Expression;
use anyhow::Result;
use pest::Parser as PestParser;
use pest_derive::Parser as DeriveParser;

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

    pub fn parse_expressions(&self) -> Result<Vec<Expression>> {
        let mut expressions: Vec<Expression> = vec![];
        let pairs = Parser::parse(Rule::program, self.source)?;

        for pair in pairs {
            match pair.as_rule() {
                Rule::get => {
                    expressions.push(Expression::Get(pair.into_inner().try_into()?));
                }
                _ => {
                    return Err(ParserError::UnexpectedToken(pair.as_str().to_string()).into());
                }
            }
        }

        Ok(expressions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::{
        account::{Account, AccountField},
        block::{Block, BlockField, BlockId, BlockRange},
        chain::{Chain, ChainOrRpc},
        dump::{Dump, DumpFormat},
        ens::NameOrAddress,
        entity::Entity,
        logs::{LogField, LogFilter, Logs},
        transaction::{Transaction, TransactionField, TransactionFilter},
        types::*,
    };
    use alloy::{
        eips::BlockNumberOrTag,
        primitives::{address, b256, Address, U128, U256},
    };
    use pretty_assertions::assert_eq;
    use std::str::FromStr;

    #[test]
    fn test_build_ast_with_account_fields() {
        let source =
            "GET nonce, balance, code FROM account 0x1234567890123456789012345678901234567890 ON eth";
        let address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
        let expected = vec![Expression::Get(GetExpression {
            entity: Entity::Account(Account::new(
                Some(vec![NameOrAddress::Address(address)]),
                None,
                vec![
                    AccountField::Nonce,
                    AccountField::Balance,
                    AccountField::Code,
                ],
            )),
            chain_or_rpc: ChainOrRpc::Chain(Chain::Ethereum),
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
            entity: Entity::Account(Account::new(
                Some(vec![NameOrAddress::Name(name)]),
                None,
                vec![AccountField::Nonce, AccountField::Balance],
            )),
            chain_or_rpc: ChainOrRpc::Chain(Chain::Ethereum),
            dump: None,
        })];
        let result = Parser::new(source).parse_expressions().unwrap();

        assert_eq!(result, expected);
    }

    #[test]
    fn test_build_get_ast_with_block_fields() {
        let source = "GET parent_hash, state_root, transactions_root, receipts_root, logs_bloom, extra_data, mix_hash, total_difficulty, base_fee_per_gas, withdrawals_root, blob_gas_used, excess_blob_gas, parent_beacon_block_root, size FROM block 1 ON eth";

        let expected = vec![Expression::Get(GetExpression {
            entity: Entity::Block(Block::new(
                Some(vec![BlockId::Number(BlockNumberOrTag::Number(1))]),
                None,
                vec![
                    BlockField::ParentHash,
                    BlockField::StateRoot,
                    BlockField::TransactionsRoot,
                    BlockField::ReceiptsRoot,
                    BlockField::LogsBloom,
                    BlockField::ExtraData,
                    BlockField::MixHash,
                    BlockField::TotalDifficulty,
                    BlockField::BaseFeePerGas,
                    BlockField::WithdrawalsRoot,
                    BlockField::BlobGasUsed,
                    BlockField::ExcessBlobGas,
                    BlockField::ParentBeaconBlockRoot,
                    BlockField::Size,
                ],
            )),
            chain_or_rpc: ChainOrRpc::Chain(Chain::Ethereum),
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
            entity: Entity::Block(Block::new(
                Some(vec![BlockId::Range(BlockRange::new(
                    BlockNumberOrTag::Number(1),
                    Some(BlockNumberOrTag::Number(2)),
                ))]),
                None,
                vec![BlockField::Timestamp],
            )),
            chain_or_rpc: ChainOrRpc::Chain(Chain::Ethereum),
            dump: None,
        })];
        let result = Parser::new(source).parse_expressions();

        match result {
            Ok(result) => assert_eq!(result, expected),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[test]
    fn test_build_get_ast_using_block_number_list() {
        let source = "GET timestamp FROM block 1,2,3 ON eth";

        let expected = vec![Expression::Get(GetExpression {
            entity: Entity::Block(Block::new(
                Some(vec![
                    BlockId::Number(BlockNumberOrTag::Number(1)),
                    BlockId::Number(BlockNumberOrTag::Number(2)),
                    BlockId::Number(BlockNumberOrTag::Number(3)),
                ]),
                None,
                vec![BlockField::Timestamp],
            )),
            chain_or_rpc: ChainOrRpc::Chain(Chain::Ethereum),
            dump: None,
        })];

        match Parser::new(source).parse_expressions() {
            Ok(result) => assert_eq!(expected, result),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[test]
    fn test_build_ast_with_transaction_fields() {
        let source = "GET transaction_type, hash, from, to, data, value, gas_price, gas, status, v, r, s, max_fee_per_blob_gas, max_fee_per_gas, max_priority_fee_per_gas, y_parity FROM tx 0x8a6a279a4d28dcc62bcb2f2a3214c93345c107b74f3081754e27471c50783f81 ON eth";

        let expected = vec![Expression::Get(GetExpression {
            entity: Entity::Transaction(Transaction::new(
                Some(vec![b256!(
                    "8a6a279a4d28dcc62bcb2f2a3214c93345c107b74f3081754e27471c50783f81"
                )]),
                None,
                vec![
                    TransactionField::TransactionType,
                    TransactionField::Hash,
                    TransactionField::From,
                    TransactionField::To,
                    TransactionField::Data,
                    TransactionField::Value,
                    TransactionField::GasPrice,
                    TransactionField::Gas,
                    TransactionField::Status,
                    TransactionField::ChainId,
                    TransactionField::V,
                    TransactionField::R,
                    TransactionField::S,
                    TransactionField::MaxFeePerBlobGas,
                    TransactionField::MaxFeePerGas,
                    TransactionField::MaxPriorityFeePerGas,
                    TransactionField::YParity,
                ],
            )),
            chain_or_rpc: ChainOrRpc::Chain(Chain::Ethereum),
            dump: None,
        })];

        match Parser::new(source).parse_expressions() {
            Ok(result) => assert_eq!(result, expected),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[test]
    fn test_build_ast_with_transaction_filter() {
        let source = "GET hash FROM tx WHERE block 1:10 ON eth";

        let expected = vec![Expression::Get(GetExpression {
            entity: Entity::Transaction(Transaction::new(
                None,
                Some(vec![TransactionFilter::BlockId(BlockId::Range(
                    BlockRange::new(
                        BlockNumberOrTag::Number(1),
                        Some(BlockNumberOrTag::Number(10)),
                    ),
                ))]),
                vec![TransactionField::Hash],
            )),
            chain_or_rpc: ChainOrRpc::Chain(Chain::Ethereum),
            dump: None,
        })];

        match Parser::new(source).parse_expressions() {
            Ok(result) => assert_eq!(result, expected),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[test]
    fn test_build_ast_from_transaction_list() {
        let source = "GET hash FROM tx 0x8a6a279a4d28dcc62bcb2f2a3214c93345c107b74f3081754e27471c50783f81, 0x12afe6797be838900c5632de516ab415addd026335461e9471dfdec17f3d4510 ON eth";

        let expected = vec![Expression::Get(GetExpression {
            entity: Entity::Transaction(Transaction::new(
                Some(vec![
                    b256!("8a6a279a4d28dcc62bcb2f2a3214c93345c107b74f3081754e27471c50783f81"),
                    b256!("12afe6797be838900c5632de516ab415addd026335461e9471dfdec17f3d4510"),
                ]),
                None,
                vec![TransactionField::Hash],
            )),
            chain_or_rpc: ChainOrRpc::Chain(Chain::Ethereum),
            dump: None,
        })];

        match Parser::new(source).parse_expressions() {
            Ok(result) => assert_eq!(result, expected),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[test]
    fn test_build_ast_with_dump() {
        let source = "GET balance FROM account vitalik.eth ON eth > vitalik-balance.csv";

        let expected = vec![Expression::Get(GetExpression {
            entity: Entity::Account(Account::new(
                Some(vec![NameOrAddress::Name("vitalik.eth".to_string())]),
                None,
                vec![AccountField::Balance],
            )),
            chain_or_rpc: ChainOrRpc::Chain(Chain::Ethereum),
            dump: Some(Dump::new("vitalik-balance".to_string(), DumpFormat::Csv)),
        })];

        match Parser::new(source).parse_expressions() {
            Ok(result) => assert_eq!(result, expected),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[test]
    fn test_build_ast_with_log_fields() {
        let source = "GET address, topic0, topic1, topic2, topic3, data, block_hash, block_number, block_timestamp, transaction_hash, transaction_index, log_index, removed FROM log WHERE block 4638757, address 0xdAC17F958D2ee523a2206206994597C13D831ec7, topic0 0xcb8241adb0c3fdb35b70c24ce35c5eb0c17af7431c99f827d44a445ca624176a ON eth,
        GET address FROM log WHERE block_hash 0xedb7f4a64744594838f7d9888883ae964fcb4714f6fe5cafb574d3ed6141ad5b, event_signature Transfer(address,address,uint256), topic1 0x00000000000000000000000036928500Bc1dCd7af6a2B4008875CC336b927D57, topic2 0x000000000000000000000000C6CDE7C39eB2f0F0095F41570af89eFC2C1Ea828 ON eth
        ";
        // let source_1 = "";

        let expected = vec![
            Expression::Get(GetExpression {
                entity: Entity::Logs(Logs::new(
                    vec![
                        LogFilter::BlockRange(BlockRange::new(
                            BlockNumberOrTag::Number(4638757),
                            None,
                        )),
                        LogFilter::EmitterAddress(address!(
                            "dac17f958d2ee523a2206206994597c13d831ec7"
                        )),
                        LogFilter::Topic0(b256!(
                            "cb8241adb0c3fdb35b70c24ce35c5eb0c17af7431c99f827d44a445ca624176a"
                        )),
                    ],
                    vec![
                        LogField::Address,
                        LogField::Topic0,
                        LogField::Topic1,
                        LogField::Topic2,
                        LogField::Topic3,
                        LogField::Data,
                        LogField::BlockHash,
                        LogField::BlockNumber,
                        LogField::BlockTimestamp,
                        LogField::TransactionHash,
                        LogField::TransactionIndex,
                        LogField::LogIndex,
                        LogField::Removed,
                    ],
                )),
                chain_or_rpc: ChainOrRpc::Chain(Chain::Ethereum),
                dump: None,
            }),
            Expression::Get(GetExpression {
                entity: Entity::Logs(Logs::new(
                    vec![
                        LogFilter::BlockHash(b256!(
                            "edb7f4a64744594838f7d9888883ae964fcb4714f6fe5cafb574d3ed6141ad5b"
                        )),
                        LogFilter::EventSignature(String::from(
                            "Transfer(address,address,uint256)",
                        )),
                        LogFilter::Topic1(b256!(
                            "00000000000000000000000036928500bc1dcd7af6a2b4008875cc336b927d57"
                        )),
                        LogFilter::Topic2(b256!(
                            "000000000000000000000000c6cde7c39eb2f0f0095f41570af89efc2c1ea828"
                        )),
                    ],
                    vec![LogField::Address],
                )),
                chain_or_rpc: ChainOrRpc::Chain(Chain::Ethereum),
                dump: None,
            }),
        ];

        match Parser::new(source).parse_expressions() {
            Ok(result) => assert_eq!(result, expected),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[test]
    fn test_build_ast_with_rpc_url() {
        let source = "GET nonce, balance FROM account 0x1234567890123456789012345678901234567890 ON http://localhost:8545";
        let address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
        let expected = vec![Expression::Get(GetExpression {
            entity: Entity::Account(Account::new(
                Some(vec![NameOrAddress::Address(address)]),
                None,
                vec![AccountField::Nonce, AccountField::Balance],
            )),
            chain_or_rpc: ChainOrRpc::Rpc("http://localhost:8545".parse().unwrap()),
            dump: None,
        })];

        match Parser::new(source).parse_expressions() {
            Ok(result) => assert_eq!(result, expected),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[test]
    fn test_build_ast_with_transaction_filter_from_ens() {
        let source = "GET * FROM tx WHERE \
            value 10000000, \
            block 10000000, \
            gas 10000000, \
            gas_price 10000000, \
            max_fee_per_blob_gas 10000000, \
            max_fee_per_gas 10000000, \
            max_priority_fee_per_gas 10000000, \
            y_parity false \
            ON eth";

        let expected = vec![Expression::Get(GetExpression {
            entity: Entity::Transaction(Transaction::new(
                None,
                Some(vec![
                    TransactionFilter::Value(U256::from(10000000)),
                    TransactionFilter::BlockId(BlockId::Range(BlockRange::new(
                        BlockNumberOrTag::Number(10000000),
                        None,
                    ))),
                    TransactionFilter::Gas(10000000.try_into().unwrap()),
                    TransactionFilter::GasPrice(10000000.try_into().unwrap()),
                    TransactionFilter::MaxFeePerBlobGas(10000000.try_into().unwrap()),
                    TransactionFilter::MaxFeePerGas(10000000.try_into().unwrap()),
                    TransactionFilter::MaxPriorityFeePerGas(10000000.try_into().unwrap()),
                    TransactionFilter::YParity(false),
                ]),
                TransactionField::all_variants().to_vec(),
            )),
            chain_or_rpc: ChainOrRpc::Chain(Chain::Ethereum),
            dump: None,
        })];

        match Parser::new(source).parse_expressions() {
            Ok(result) => assert_eq!(result, expected),
            Err(e) => panic!("Error: {}", e),
        }
    }
}
