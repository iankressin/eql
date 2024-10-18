use super::{
    resolve_account::resolve_account_query,
    resolve_block::resolve_block_query,
    resolve_logs::resolve_log_query,
    resolve_transaction::resolve_transaction_query,
};
use crate::common::{
    entity::Entity, query_result::{ExpressionResult, QueryResult}, serializer::dump_results, types::{Expression, GetExpression}
};
use alloy::providers::ProviderBuilder;
use std::{error::Error, sync::Arc};

pub struct ExecutionEngine;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ExecutionEngineError {
    #[error("Neither an entity_id nor a filter was provided. Pest rules should have prevented this from happening.")]
    NoEntityIdOrFilter,
    #[error("Multiple filters are not supported for block queries.")]
    MultipleFiltersNotSupported,
}

impl ExecutionEngine {
    pub fn new() -> ExecutionEngine {
        ExecutionEngine
    }

    pub async fn run(
        &self,
        expressions: Vec<Expression>,
    ) -> Result<Vec<QueryResult>, Box<dyn Error>> {
        let mut query_results = vec![];

        for expression in expressions {
            match expression {
                Expression::Get(get_expr) => {
                    let result = self.run_get_expr(&get_expr).await?;
                    query_results.push(QueryResult::new(result));
                }
            }
        }

        Ok(query_results)
    }

    async fn run_get_expr(
        &self,
        expr: &GetExpression,
    ) -> Result<ExpressionResult, Box<dyn std::error::Error>> {
        let rpc_url = expr.chain_or_rpc.rpc_url()?;
        let provider = Arc::new(ProviderBuilder::new().on_http(rpc_url.clone()));

        let result = match &expr.entity {
            Entity::Block(block) => Ok(ExpressionResult::Block(resolve_block_query(block, provider.clone()).await?)),
            Entity::Account(account) => Ok(ExpressionResult::Account(resolve_account_query(account, provider.clone()).await?)),
            Entity::Transaction(transaction) => Ok(ExpressionResult::Transaction(resolve_transaction_query(transaction, provider.clone()).await?)),
            Entity::Logs(logs) => Ok(ExpressionResult::Log(resolve_log_query(logs, provider.clone()).await?)),
        };

        result.and_then(|result| {
            expr.dump.as_ref().map(|dump| dump_results(&result, dump));
            Ok(result)
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::common::{
        account::{Account, AccountField},
        block::{Block, BlockField, BlockId, BlockRange},
        chain::{Chain, ChainOrRpc},
        dump::{Dump, DumpFormat},
        ens::NameOrAddress,
        logs::{LogField, LogFilter, Logs},
        query_result::{BlockQueryRes, LogQueryRes, TransactionQueryRes},
        transaction::{Transaction, TransactionField},
        types::{Expression, GetExpression},
    };
    use alloy::{
        eips::BlockNumberOrTag,
        primitives::{address, b256, bloom, bytes, U256},
    };
    use pretty_assertions::assert_eq;
    use std::str::FromStr;

    #[tokio::test]
    async fn test_get_logs() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            entity: Entity::Logs(Logs::new(
                vec![
                    LogFilter::BlockRange(BlockRange::new(
                        BlockNumberOrTag::Number(4638757),
                        Some(BlockNumberOrTag::Number(4638758)),
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
                ],
            )),
            chain_or_rpc: ChainOrRpc::Chain(Chain::Ethereum),
            dump: None,
        })];
        let execution_result = execution_engine.run(expressions).await;
        let expected = vec![LogQueryRes {
            address: Some(address!("dac17f958d2ee523a2206206994597c13d831ec7")),
            topic0: Some(b256!(
                "cb8241adb0c3fdb35b70c24ce35c5eb0c17af7431c99f827d44a445ca624176a"
            )),
            topic1: None,
            topic2: None,
            topic3: None,
            data: Some(bytes!(
                "00000000000000000000000000000000000000000000000000000002540be400"
            )),
            block_hash: Some(b256!(
                "d34e3b2957865fe76c73ec91d798f78de95f2b0e0cddfc47e341b5f235dc4d58"
            )),
            block_number: Some(4638757),
            block_timestamp: Some(1511886266),
            transaction_hash: Some(b256!(
                "8cfc4f5f4729423f59dd1d263ead2f824b3f133b02b9e27383964c7d50cd47cb"
            )),
            transaction_index: Some(9),
            log_index: Some(5),
            removed: None,
        }];

        match execution_result {
            Ok(results) => {
                assert_eq!(results[0].result, ExpressionResult::Log(expected));
            }
            Err(_) => panic!("Error"),
        }
    }

    #[tokio::test]
    async fn test_get_block_fields() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            entity: Entity::Block(
                Block::new(
                    Some(vec![
                        BlockId::Range(BlockRange::new(
                            BlockNumberOrTag::Number(1),
                            None,
                        )),
                    ]),
                    None,
                    vec![
                        BlockField::Timestamp,
                        BlockField::Hash,
                        BlockField::ParentHash,
                        BlockField::Size,
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
                    ],
                )
            ),
            dump: None,
            chain_or_rpc: ChainOrRpc::Chain(Chain::Ethereum),
        })];
        let expected = ExpressionResult::Block(vec![
            BlockQueryRes {
                timestamp: Some(1438269988),
                number: None,
                hash: Some(b256!(
                    "88e96d4537bea4d9c05d12549907b32561d3bf31f45aae734cdc119f13406cb6"
                )),
                parent_hash: Some(b256!(
                    "d4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3"
                )),
                size: Some(U256::from(537)),
                state_root: Some(b256!(
                    "d67e4d450343046425ae4271474353857ab860dbc0a1dde64b41b5cd3a532bf3"
                )),
                transactions_root: Some(b256!(
                    "56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
                )),
                receipts_root: Some(b256!(
                    "56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
                )),
                logs_bloom: Some(bloom!("00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000")),
                extra_data: Some(bytes!("476574682f76312e302e302f6c696e75782f676f312e342e32")),
                mix_hash: Some(b256!("969b900de27b6ac6a67742365dd65f55a0526c41fd18e1b16f1a1215c2e66f59")),
                total_difficulty: Some(U256::from(34351349760_u128)),
                // The fields below were implemented by EIPs, 1st block doesn't have these
                base_fee_per_gas: None,
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                parent_beacon_block_root: None,
            },
        ]);
        let execution_result = execution_engine.run(expressions).await;

        match execution_result {
            Ok(results) => {
                assert_eq!(results[0].result, expected);
            }
            Err(_) => panic!("Error"),
        }
    }

    #[tokio::test]
    async fn test_get_account_fields_using_invalid_ens() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            entity: Entity::Account(
                Account::new(
                    Some(vec![NameOrAddress::Name(String::from("thisisinvalid235790123801.eth"))]),
                    None,
                    vec![AccountField::Balance],
                )
            ),
            chain_or_rpc: ChainOrRpc::Chain(Chain::Ethereum),
            dump: None,
        })];
        let execution_result = execution_engine.run(expressions).await;
        assert!(execution_result.is_err())
    }

    #[tokio::test]
    async fn test_get_transaction_fields() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            entity: Entity::Transaction(
                Transaction::new(
                    Some(vec![
                        b256!("72546b3ca8ef0dfb85fe66d19645e44cb519858c72fbcad0e1c1699256fed890"),
                        b256!("72546b3ca8ef0dfb85fe66d19645e44cb519858c72fbcad0e1c1699256fed890")
                    ]),
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
                    ]
                )
            ),
            chain_or_rpc: ChainOrRpc::Chain(Chain::Ethereum),
            dump: None,
        })];
        let expected = vec![ExpressionResult::Transaction(vec![
            TransactionQueryRes {
                transaction_type: Some(2),
                hash: Some(b256!(
                    "72546b3ca8ef0dfb85fe66d19645e44cb519858c72fbcad0e1c1699256fed890"
                )),
                from: Some(address!("95222290dd7278aa3ddd389cc1e1d165cc4bafe5")),
                to: Some(address!("2eeb301387d6bda23e02fa0c7463507c68b597b5")),
                data: Some(bytes!("")),
                value: Some(U256::from(234808500010631948_u128)),
                gas_price: Some(10209184711_u128),
                gas: Some(21000),
                status: Some(true),
                chain_id: Some(1),
                v: Some(U256::from(0)),
                r: Some(U256::from_str("105656622829170817033829205634607968479218860016837137132236076370603621041980").unwrap()),
                s: Some(U256::from_str("15038977765364444198936700207894720753481416564436657360670639019817488048130").unwrap()),
                max_fee_per_blob_gas: None,
                max_fee_per_gas: Some(10209184711),
                max_priority_fee_per_gas: Some(0),
                y_parity: Some(false),
            },
            TransactionQueryRes {
                transaction_type: Some(2),
                hash: Some(b256!(
                    "72546b3ca8ef0dfb85fe66d19645e44cb519858c72fbcad0e1c1699256fed890"
                )),
                from: Some(address!("95222290dd7278aa3ddd389cc1e1d165cc4bafe5")),
                to: Some(address!("2eeb301387d6bda23e02fa0c7463507c68b597b5")),
                data: Some(bytes!("")),
                value: Some(U256::from(234808500010631948_u128)),
                gas_price: Some(10209184711_u128),
                gas: Some(21000),
                status: Some(true),
                chain_id: Some(1),
                v: Some(U256::from(0)),
                r: Some(U256::from_str("105656622829170817033829205634607968479218860016837137132236076370603621041980").unwrap()),
                s: Some(U256::from_str("15038977765364444198936700207894720753481416564436657360670639019817488048130").unwrap()),
                max_fee_per_blob_gas: None,
                max_fee_per_gas: Some(10209184711),
                max_priority_fee_per_gas: Some(0),
                y_parity: Some(false),
            }])    
        ];            

        let result = execution_engine.run(expressions).await;
        match result {
            Ok(results) => {
                assert_eq!(results[0].result, expected[0]);
            }
            Err(_) => panic!("Error"),
        }
    }

    #[tokio::test]
    #[should_panic]
    async fn test_get_transaction_fields_does_not_exist() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            entity: Entity::Transaction(
                Transaction::new(
                    Some(vec![b256!(
                        "bebd3baab326f895289ecbd4210cf886ce41952316441ae4cac35f00f0e882a6"
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
                    ]
                )
            ),
            chain_or_rpc: ChainOrRpc::Chain(Chain::Ethereum),
            dump: None,
        })];
        let _result = execution_engine.run(expressions).await;
    }

    #[tokio::test]
    async fn test_dump_results() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            entity: Entity::Block(
                Block::new(
                    Some(vec![
                        BlockId::Range(BlockRange::new(
                            1.into(),
                            None,
                        ))
                    ]),
                    None,
                    vec![BlockField::Timestamp],
                )
            ),
            chain_or_rpc: ChainOrRpc::Chain(Chain::Ethereum),
            dump: Some(Dump::new(String::from("test"), DumpFormat::Json)),
        })];
        execution_engine.run(expressions).await.unwrap();

        let path = std::path::Path::new("test.json");
        let expected_content = r#"
        {
            "block": [
                {
                    "timestamp": 1438269988
                }
            ]
        }"#;

        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(flatten_string(&content), flatten_string(expected_content));

        std::fs::remove_file(&path).unwrap();
    }

    fn flatten_string(s: &str) -> String {
        s.replace('\n', "").replace('\r', "").replace(" ", "")
    }
}
