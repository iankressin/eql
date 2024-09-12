use super::{
    resolve_account::resolve_account_query,
    resolve_block::resolve_block_query,
    resolve_logs::resolve_log_query,
    resolve_transaction::resolve_transaction_query,
};
use crate::common::{
    entity::Entity, entity_filter::EntityFilter, entity_id::EntityId, query_result::{ExpressionResult, QueryResult}, serializer::dump_results, types::{AccountField, BlockField, Expression, Field, GetExpression, LogField, TransactionField}
};
use alloy::providers::ProviderBuilder;
use std::error::Error;

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
                    query_results.push(QueryResult::new(get_expr.query, result));
                }
            }
        }

        Ok(query_results)
    }

    async fn run_get_expr(
        &self,
        expr: &GetExpression,
    ) -> Result<ExpressionResult, Box<dyn std::error::Error>> {
        let rpc_url = expr.chain.rpc_url().parse()?;
        let provider = ProviderBuilder::new().on_http(rpc_url);

        let result = match expr.entity {
            Entity::Block => {
                let fields = match &expr.fields[0] {
                    Field::Star => BlockField::all_variants().to_vec(),
                    _ => expr
                        .fields
                        .iter()
                        .map(|field| field.try_into())
                        .collect::<Result<Vec<BlockField>, _>>()?,
                };

                let block_query_res = if let Some(entity_ids) = &expr.entity_id {
                        resolve_block_query(entity_ids.to_vec(), fields, &provider).await?
                } 
                else if let Some(entity_filter) = &expr.entity_filter {
                    match entity_filter.as_slice() {
                        [EntityFilter::LogBlockRange(range)] => {
                            let entity_ids = vec![EntityId::Block(range.clone())];
                            resolve_block_query(entity_ids, fields, &provider).await?
                        }
                        _ => {
                            return Err(Box::new(ExecutionEngineError::MultipleFiltersNotSupported));
                        }
                    }
                } 
                else {
                    return Err(Box::new(ExecutionEngineError::NoEntityIdOrFilter));
                };

                Ok(ExpressionResult::Block(block_query_res))
            }

            Entity::Account => {
                let fields = match expr.fields[0] {
                    Field::Star => AccountField::all_variants().to_vec(),
                    _ => expr
                        .fields
                        .iter()
                        .map(|field| field.try_into())
                        .collect::<Result<Vec<AccountField>, _>>()?,
                };             
                
                let account_query_res = if let Some(entity_ids) = &expr.entity_id {
                    resolve_account_query(entity_ids.to_vec(), fields, &provider).await?
                } else {
                    return Err(Box::new(ExecutionEngineError::NoEntityIdOrFilter));
                };

                Ok(ExpressionResult::Account(account_query_res))
            }

            Entity::Transaction => {                
                let fields = match expr.fields[0] {
                    Field::Star => TransactionField::all_variants().to_vec(),
                    _ => expr
                        .fields
                        .iter()
                        .map(|field| field.try_into())
                        .collect::<Result<Vec<TransactionField>, _>>()?,
                };
                
                let tx_query_res = if let Some(entity_ids) = &expr.entity_id {
                    resolve_transaction_query(entity_ids.to_vec(), fields, &provider).await?   
                } else {
                    return Err(Box::new(ExecutionEngineError::NoEntityIdOrFilter));
                };

                Ok(ExpressionResult::Transaction(tx_query_res))
            }
            
            Entity::Log => {                
                let fields = match expr.fields[0] {
                    Field::Star => LogField::all_variants().to_vec(),
                    _ => expr
                        .fields
                        .iter()
                        .map(|field| field.try_into())
                        .collect::<Result<Vec<LogField>, _>>()?,
                };

                let filter = if let Some(entity_filter) = &expr.entity_filter {
                    EntityFilter::build_filter(entity_filter)
                } else {
                    return Err(Box::new(ExecutionEngineError::NoEntityIdOrFilter));
                };

                Ok(ExpressionResult::Log(resolve_log_query(filter, fields, &provider).await?))
            }
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
        chain::Chain,
        ens::NameOrAddress,
        entity_filter::{BlockRange, EntityFilter},
        entity_id::EntityId,
        query_result::{BlockQueryRes, LogQueryRes, TransactionQueryRes},
        types::{BlockField, Dump, DumpFormat, Expression, Field, GetExpression},
    };
    use alloy::{
        eips::BlockNumberOrTag,
        primitives::{address, b256, bloom, bytes, Address, U256},
    };
    use pretty_assertions::assert_eq;
    use std::str::FromStr;

    #[tokio::test]
    async fn test_get_logs() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            chain: Chain::Ethereum,
            entity: Entity::Log,
            entity_id: None,
            entity_filter: Some(vec![
                EntityFilter::LogBlockRange(BlockRange::new(
                    BlockNumberOrTag::Number(4638757),
                    Some(BlockNumberOrTag::Number(4638758)),
                )),
                EntityFilter::LogEmitterAddress(address!(
                    "dac17f958d2ee523a2206206994597c13d831ec7"
                )),
                EntityFilter::LogTopic0(b256!(
                    "cb8241adb0c3fdb35b70c24ce35c5eb0c17af7431c99f827d44a445ca624176a"
                )),
            ]),
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
            ],
            query: String::from(""),
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
            chain: Chain::Ethereum,
            entity: Entity::Block,
            entity_id: Some(vec![
                EntityId::Block(BlockRange::new(BlockNumberOrTag::Number(1), None)),
                EntityId::Block(BlockRange::new(BlockNumberOrTag::Number(1), None))
                ]),
            entity_filter: None,
            fields: vec![
                Field::Block(BlockField::Timestamp),
                Field::Block(BlockField::Hash),
                Field::Block(BlockField::ParentHash),
                Field::Block(BlockField::Size),
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
            ],
            query: String::from(""),
            dump: None,
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
    async fn test_error_when_id_doesnt_match_transaction() {
        let fields = vec![]; // Empty fields for simplicity
        let provider = ProviderBuilder::new().on_http(Chain::Sepolia.rpc_url().parse().unwrap());
        let entity_id = EntityId::Block(BlockRange::new(
            10.into(),
            Some(BlockNumberOrTag::from_str("latest").unwrap()),
        ));

        let result = resolve_transaction_query(vec![entity_id], fields, &provider)
            .await
            .unwrap_err()
            .to_string();

        assert_eq!(
            result,
            "Mismatch between Entity and EntityId, 10:latest can't be resolved as a transaction id"
        );
    }

    #[tokio::test]
    async fn test_get_account_fields_using_star_operator() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            chain: Chain::Ethereum,
            entity: Entity::Account,
            entity_id: Some(vec![
                EntityId::Account(NameOrAddress::Address(Address::from_str("0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045").unwrap())),
                EntityId::Account(NameOrAddress::Name(String::from("vitalik.eth")))
                ]),
            entity_filter: None,
            fields: vec![Field::Star],
            query: String::from(""),
            dump: None,
        })];
        let execution_result = execution_engine.run(expressions).await;

        match execution_result {
            Ok(results) => match &results[0] {
                QueryResult { query, result, .. } => {
                    assert_eq!(query, "");
                    match result {
                        ExpressionResult::Account(account) => {
                            assert!(account[0].balance.is_some());
                        }
                        _ => panic!("Invalid result"),
                    }
                }
            },
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_get_account_fields_using_invalid_ens() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            chain: Chain::Ethereum,
            entity: Entity::Account,
            entity_id: Some(vec![
                EntityId::Account(NameOrAddress::Name(String::from("thisisinvalid235790123801.eth")))
                ]),
            entity_filter: None,
            fields: vec![Field::Account(AccountField::Balance)],
            query: String::from(""),
            dump: None,
        })];
        let execution_result = execution_engine.run(expressions).await;
        assert!(execution_result.is_err())
    }

    #[tokio::test]
    async fn test_get_transaction_fields() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            chain: Chain::Ethereum,
            entity: Entity::Transaction,
            entity_id: Some(vec![
                EntityId::Transaction(b256!("72546b3ca8ef0dfb85fe66d19645e44cb519858c72fbcad0e1c1699256fed890")),
                EntityId::Transaction(b256!("72546b3ca8ef0dfb85fe66d19645e44cb519858c72fbcad0e1c1699256fed890"))
                ]),
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
            query: String::from(""),
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
            chain: Chain::Ethereum,
            entity: Entity::Transaction,
            entity_id: Some(vec![EntityId::Transaction(b256!(
                "bebd3baab326f895289ecbd4210cf886ce41952316441ae4cac35f00f0e882a6"
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
            query: String::from(""),
            dump: None,
        })];
        let _result = execution_engine.run(expressions).await;
    }

    #[tokio::test]
    async fn test_dump_results() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            chain: Chain::Ethereum,
            entity: Entity::Block,
            entity_id: Some(vec![
                EntityId::Block(BlockRange::new(1.into(), None)),
            ]),
            entity_filter: None,
            fields: vec![Field::Block(BlockField::Timestamp)],
            query: String::from(""),
            dump: Some(Dump::new(String::from("test"), DumpFormat::Json)),
        })];
        execution_engine.run(expressions).await.unwrap();

        let path = std::path::Path::new("test.json").to_str();
        let expected_content = r#"
        {
            "block": [
                {
                    "timestamp": 1438269988
                }
            ]
        }"#;

        println!("{:?}", path);

        assert!(std::path::Path::new("test.json").exists());

        let content = std::fs::read_to_string("test.json").unwrap();
        assert_eq!(flatten_string(&content), flatten_string(expected_content));

        std::fs::remove_file("test.json").unwrap();
    }

    fn flatten_string(s: &str) -> String {
        s.replace('\n', "").replace('\r', "").replace(" ", "")
    }
}
