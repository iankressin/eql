use super::{
    resolve_account::resolve_account_query, resolve_block::resolve_block_query,
    resolve_logs::resolve_log_query, resolve_transaction::resolve_transaction_query,
};
use crate::common::{
    entity::Entity,
    query_result::{ExpressionResult, QueryResult},
    serializer::{dump_results, dump_results_with_aliases},
    types::{Expression, GetExpression},
};
use crate::interpreter::frontend::sql::EqlSqlError;
use anyhow::Result;

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

    pub async fn run(&self, expressions: Vec<Expression>) -> Result<Vec<QueryResult>> {
        let mut query_results = vec![];

        for expression in expressions {
            match expression {
                Expression::Get(get_expr) => {
                    let result = self.run_get_expr(&get_expr).await?;
                    query_results.push(QueryResult::new(result));
                }
                // `SET rpc_<chain> = '<url>'` applies a session-scoped RPC
                // override rather than resolving into rows, so it produces
                // no `QueryResult` (see `Config::set_session_rpc`'s and
                // `Chain::rpc_url`'s doc comments for what "session-scoped"
                // means in practice).
                Expression::Set(set_expr) => {
                    crate::common::config::Config::set_session_rpc(&set_expr.chain, set_expr.url);
                }
            }
        }

        Ok(query_results)
    }

    async fn run_get_expr(&self, expr: &GetExpression) -> Result<ExpressionResult> {
        let mut result = match &expr.entity {
            Entity::Block(block) => {
                ExpressionResult::Block(resolve_block_query(block, &expr.chains).await?)
            }
            Entity::Account(account) => {
                ExpressionResult::Account(resolve_account_query(account, &expr.chains).await?)
            }
            Entity::Transaction(transaction) => ExpressionResult::Transaction(
                resolve_transaction_query(transaction, &expr.chains).await?,
            ),
            Entity::Logs(logs) => {
                ExpressionResult::Log(resolve_log_query(logs, &expr.chains).await?)
            }
        };

        // v1 shape: rows for every chain in `expr.chains` are already
        // flattened into `result` by the resolvers above, so `LIMIT` caps
        // the combined row count across all chains, not per chain. It also
        // truncates after the full fetch rather than pushing the limit down
        // to Portal.
        if let Some(limit) = expr.limit {
            result.truncate(limit);
        }

        if let Some(dump) = &expr.dump {
            match (&expr.aliases, &dump.format) {
                (Some(aliases), crate::common::dump::DumpFormat::Json) => {
                    dump_results_with_aliases(&result, dump, aliases)?;
                }
                (Some(_), other_format) => {
                    return Err(EqlSqlError::NotSupported(format!(
                        "AS aliases with {other_format} exports"
                    ))
                    .into());
                }
                // No aliases: same write path as the aliased branch above —
                // `COPY`'s entire purpose is the file write, so a failed
                // write (full disk, bad path, ...) must fail the query
                // rather than silently report success. `dump_results`
                // returns `Box<dyn Error>`, which doesn't implement
                // `std::error::Error` itself, so it can't cross a bare `?`
                // into `anyhow::Result`; convert it explicitly.
                (None, _) => {
                    dump_results(&result, dump)
                        .map_err(|e| anyhow::anyhow!("failed to write export: {e}"))?;
                }
            }
        }

        Ok(result)
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
        query_result::{AccountQueryRes, BlockQueryRes, LogQueryRes, TransactionQueryRes},
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
                    LogFilter::EmitterAddress(address!("dac17f958d2ee523a2206206994597c13d831ec7")),
                    LogFilter::Topic0(b256!(
                        "cb8241adb0c3fdb35b70c24ce35c5eb0c17af7431c99f827d44a445ca624176a"
                    )),
                ],
                LogField::all_variants().to_vec(),
            )),
            chains: vec![ChainOrRpc::Chain(Chain::Ethereum)],
            dump: None,
            limit: None,
            aliases: None,
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
            removed: Some(false),
            chain: Some(Chain::Ethereum),
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
            entity: Entity::Block(Block::new(
                Some(vec![BlockId::Range(BlockRange::new(
                    BlockNumberOrTag::Number(1),
                    None,
                ))]),
                None,
                BlockField::all_variants().to_vec(),
            )),
            dump: None,
            limit: None,
            aliases: None,
            chains: vec![ChainOrRpc::Chain(Chain::Ethereum)],
        })];
        let expected = ExpressionResult::Block(vec![
            BlockQueryRes {
                timestamp: Some(1438269988),
                number: Some(1),
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
                chain: Some(Chain::Ethereum),
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
            entity: Entity::Account(Account::new(
                Some(vec![NameOrAddress::Name(String::from(
                    "thisisinvalid235790123801.eth",
                ))]),
                None,
                vec![AccountField::Balance],
            )),
            chains: vec![ChainOrRpc::Chain(Chain::Ethereum)],
            dump: None,
            limit: None,
            aliases: None,
        })];
        let execution_result = execution_engine.run(expressions).await;
        assert!(execution_result.is_err())
    }

    #[tokio::test]
    async fn test_get_transaction_fields() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            entity: Entity::Transaction(Transaction::new(
                Some(vec![
                    b256!("72546b3ca8ef0dfb85fe66d19645e44cb519858c72fbcad0e1c1699256fed890"),
                    b256!("72546b3ca8ef0dfb85fe66d19645e44cb519858c72fbcad0e1c1699256fed890"),
                ]),
                None,
                TransactionField::all_variants().to_vec(),
            )),
            chains: vec![ChainOrRpc::Chain(Chain::Ethereum)],
            dump: None,
            limit: None,
            aliases: None,
        })];
        let expected = vec![ExpressionResult::Transaction(vec![
            TransactionQueryRes {
                r#type: Some(2),
                hash: Some(b256!(
                    "72546b3ca8ef0dfb85fe66d19645e44cb519858c72fbcad0e1c1699256fed890"
                )),
                block_number: Some(20183336),
                from_address: Some(address!("95222290dd7278aa3ddd389cc1e1d165cc4bafe5")),
                to_address: Some(address!("2eeb301387d6bda23e02fa0c7463507c68b597b5")),
                data: Some(bytes!("")),
                value: Some(U256::from(234808500010631948_u128)),
                gas_price: None,
                gas_limit: Some(21000),
                effective_gas_price: Some(10209184711_u128),
                status: Some(true),
                chain_id: Some(1),
                v: Some(false),
                r: Some(U256::from_str("105656622829170817033829205634607968479218860016837137132236076370603621041980").unwrap()),
                s: Some(U256::from_str("15038977765364444198936700207894720753481416564436657360670639019817488048130").unwrap()),
                max_fee_per_blob_gas: None,
                max_fee_per_gas: Some(10209184711),
                max_priority_fee_per_gas: Some(0),
                y_parity: Some(false),
                chain: Some(Chain::Ethereum),
                authorization_list: None,
            },
            TransactionQueryRes {
                r#type: Some(2),
                hash: Some(b256!(
                    "72546b3ca8ef0dfb85fe66d19645e44cb519858c72fbcad0e1c1699256fed890"
                )),
                block_number: Some(20183336),
                from_address: Some(address!("95222290dd7278aa3ddd389cc1e1d165cc4bafe5")),
                to_address: Some(address!("2eeb301387d6bda23e02fa0c7463507c68b597b5")),
                data: Some(bytes!("")),
                value: Some(U256::from(234808500010631948_u128)),
                gas_price: None,
                gas_limit: Some(21000),
                effective_gas_price: Some(10209184711_u128),
                status: Some(true),
                chain_id: Some(1),
                v: Some(false),
                r: Some(U256::from_str("105656622829170817033829205634607968479218860016837137132236076370603621041980").unwrap()),
                s: Some(U256::from_str("15038977765364444198936700207894720753481416564436657360670639019817488048130").unwrap()),
                max_fee_per_blob_gas: None,
                max_fee_per_gas: Some(10209184711),
                max_priority_fee_per_gas: Some(0),
                y_parity: Some(false),
                chain: Some(Chain::Ethereum),
                authorization_list: None,
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
    async fn test_get_transactions_via_portal_block_range() {
        use crate::common::filters::EqualityFilter;
        use crate::common::transaction::TransactionFilter;

        let execution_engine = ExecutionEngine::new();
        // A single concrete block, filtered by sender, GET * -> must route through Portal.
        let expressions = vec![Expression::Get(GetExpression {
            entity: Entity::Transaction(Transaction::new(
                None,
                Some(vec![
                    TransactionFilter::BlockId(BlockId::Range(BlockRange::new(
                        BlockNumberOrTag::Number(20000000),
                        Some(BlockNumberOrTag::Number(20000000)),
                    ))),
                    TransactionFilter::From(EqualityFilter::Eq(address!(
                        "95222290dd7278aa3ddd389cc1e1d165cc4bafe5"
                    ))),
                ]),
                TransactionField::all_variants().to_vec(),
            )),
            chains: vec![ChainOrRpc::Chain(Chain::Ethereum)],
            dump: None,
            limit: None,
            aliases: None,
        })];

        let result = execution_engine.run(expressions).await.unwrap();
        match &result[0].result {
            ExpressionResult::Transaction(txs) => {
                assert!(!txs.is_empty(), "expected at least one tx from Portal");
                // authorization_list is always None on the Portal path.
                assert!(txs.iter().all(|t| t.authorization_list.is_none()));
                // GET * populates hash + from_address on every row.
                assert!(txs
                    .iter()
                    .all(|t| t.hash.is_some() && t.from_address.is_some()));
                // Portal forces fields.block.number on for pagination; every row in this
                // single-block range should carry that block's number.
                assert!(txs.iter().all(|t| t.block_number == Some(20000000)));
            }
            other => panic!("expected Transaction result, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_get_transactions_via_portal_authorization_list_only_retains_rows() {
        use crate::common::transaction::TransactionFilter;

        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            entity: Entity::Transaction(Transaction::new(
                None,
                Some(vec![TransactionFilter::BlockId(BlockId::Range(
                    BlockRange::new(
                        BlockNumberOrTag::Number(20000000),
                        Some(BlockNumberOrTag::Number(20000000)),
                    ),
                ))]),
                vec![TransactionField::AuthorizationList],
            )),
            chains: vec![ChainOrRpc::Chain(Chain::Ethereum)],
            dump: None,
            limit: None,
            aliases: None,
        })];

        let result = execution_engine.run(expressions).await.unwrap();
        match &result[0].result {
            ExpressionResult::Transaction(txs) => {
                assert!(
                    !txs.is_empty(),
                    "expected Portal to retain rows whose only projected field is None"
                );
                assert!(txs.iter().all(|tx| tx.authorization_list.is_none()));
            }
            other => panic!("expected Transaction result, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_get_inexistent_transaction() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            entity: Entity::Transaction(Transaction::new(
                Some(vec![b256!(
                    "0000000000000000000000000000000000000000000000000000000000000000"
                )]),
                None,
                TransactionField::all_variants().to_vec(),
            )),
            chains: vec![ChainOrRpc::Chain(Chain::Ethereum)],
            dump: None,
            limit: None,
            aliases: None,
        })];
        let result = execution_engine.run(expressions).await.unwrap();

        assert_eq!(result[0].result, ExpressionResult::Transaction(vec![]));
    }

    #[tokio::test]
    async fn test_dump_results() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            entity: Entity::Block(Block::new(
                Some(vec![BlockId::Range(BlockRange::new(1.into(), None))]),
                None,
                vec![BlockField::Timestamp],
            )),
            chains: vec![ChainOrRpc::Chain(Chain::Ethereum)],
            dump: Some(Dump::new(String::from("test"), DumpFormat::Json)),
            limit: None,
            aliases: None,
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

    #[tokio::test]
    async fn test_limit_truncates_the_combined_result() {
        // Blocks 1-3 resolve to 3 rows on a single chain; `limit: Some(2)`
        // must cap that to 2 regardless of how many rows were fetched.
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            entity: Entity::Block(Block::new(
                Some(vec![BlockId::Range(BlockRange::new(
                    BlockNumberOrTag::Number(1),
                    Some(BlockNumberOrTag::Number(3)),
                ))]),
                None,
                vec![BlockField::Number],
            )),
            chains: vec![ChainOrRpc::Chain(Chain::Ethereum)],
            dump: None,
            limit: Some(2),
            aliases: None,
        })];

        let result = execution_engine.run(expressions).await.unwrap();
        match &result[0].result {
            ExpressionResult::Block(rows) => assert_eq!(rows.len(), 2),
            other => panic!("expected Block result, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_dump_results_with_aliases_renames_json_keys() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            entity: Entity::Block(Block::new(
                Some(vec![BlockId::Range(BlockRange::new(1.into(), None))]),
                None,
                vec![BlockField::Timestamp],
            )),
            chains: vec![ChainOrRpc::Chain(Chain::Ethereum)],
            dump: Some(Dump::new(String::from("test_alias_dump"), DumpFormat::Json)),
            limit: None,
            aliases: Some(std::collections::HashMap::from([(
                "timestamp".to_string(),
                "ts".to_string(),
            )])),
        })];
        execution_engine.run(expressions).await.unwrap();

        let path = std::path::Path::new("test_alias_dump.json");
        // Same top-level `{"<entity>": [...]}` shape as the unaliased dump
        // (see `test_dump_results` above) — only the inner row key differs.
        let expected_content = r#"
        {
            "block": [
                {
                    "ts": 1438269988
                }
            ]
        }"#;

        assert!(path.exists());

        let content = std::fs::read_to_string(path).unwrap();
        assert_eq!(flatten_string(&content), flatten_string(expected_content));

        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn test_alias_with_csv_dump_is_rejected_naming_csv() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            entity: Entity::Block(Block::new(
                Some(vec![BlockId::Range(BlockRange::new(1.into(), None))]),
                None,
                vec![BlockField::Timestamp],
            )),
            chains: vec![ChainOrRpc::Chain(Chain::Ethereum)],
            dump: Some(Dump::new(String::from("test_alias_csv"), DumpFormat::Csv)),
            limit: None,
            aliases: Some(std::collections::HashMap::from([(
                "timestamp".to_string(),
                "ts".to_string(),
            )])),
        })];

        let result = execution_engine.run(expressions).await;
        let err = result
            .err()
            .expect("expected AS aliases + csv to be rejected");
        assert!(
            err.to_string().contains("csv"),
            "error should name the format the user asked for, got: {err}"
        );
        assert!(!std::path::Path::new("test_alias_csv.csv").exists());
    }

    #[tokio::test]
    async fn test_alias_with_parquet_dump_is_rejected_naming_parquet() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            entity: Entity::Block(Block::new(
                Some(vec![BlockId::Range(BlockRange::new(1.into(), None))]),
                None,
                vec![BlockField::Timestamp],
            )),
            chains: vec![ChainOrRpc::Chain(Chain::Ethereum)],
            dump: Some(Dump::new(
                String::from("test_alias_parquet"),
                DumpFormat::Parquet,
            )),
            limit: None,
            aliases: Some(std::collections::HashMap::from([(
                "timestamp".to_string(),
                "ts".to_string(),
            )])),
        })];

        let result = execution_engine.run(expressions).await;
        let err = result
            .err()
            .expect("expected AS aliases + parquet to be rejected");
        assert!(
            err.to_string().contains("parquet"),
            "error should name the format the user asked for, got: {err}"
        );
        assert!(!std::path::Path::new("test_alias_parquet.parquet").exists());
    }

    #[tokio::test]
    async fn test_get_chain_field() {
        let execution_engine = ExecutionEngine::new();
        let test_cases = vec![
            (
                Expression::Get(GetExpression {
                    entity: Entity::Block(Block::new(
                        Some(vec![BlockId::Number(BlockNumberOrTag::Number(1))]),
                        None,
                        vec![BlockField::Chain],
                    )),
                    chains: vec![ChainOrRpc::Chain(Chain::Ethereum)],
                    dump: None,
                    limit: None,
                    aliases: None,
                }),
                ExpressionResult::Block(vec![BlockQueryRes {
                    chain: Some(Chain::Ethereum),
                    ..Default::default()
                }]),
            ),
            (
                Expression::Get(GetExpression {
                    entity: Entity::Account(Account::new(
                        Some(vec![NameOrAddress::Address(address!(
                            "dac17f958d2ee523a2206206994597c13d831ec7"
                        ))]),
                        None,
                        vec![AccountField::Chain],
                    )),
                    chains: vec![ChainOrRpc::Chain(Chain::Ethereum)],
                    dump: None,
                    limit: None,
                    aliases: None,
                }),
                ExpressionResult::Account(vec![AccountQueryRes {
                    chain: Some(Chain::Ethereum),
                    ..Default::default()
                }]),
            ),
            (
                Expression::Get(GetExpression {
                    entity: Entity::Transaction(Transaction::new(
                        Some(vec![b256!(
                            "72546b3ca8ef0dfb85fe66d19645e44cb519858c72fbcad0e1c1699256fed890"
                        )]),
                        None,
                        vec![TransactionField::Chain],
                    )),
                    chains: vec![ChainOrRpc::Chain(Chain::Ethereum)],
                    dump: None,
                    limit: None,
                    aliases: None,
                }),
                ExpressionResult::Transaction(vec![TransactionQueryRes {
                    chain: Some(Chain::Ethereum),
                    ..Default::default()
                }]),
            ),
        ];

        for (expression, expected) in test_cases {
            let result = execution_engine.run(vec![expression]).await.unwrap();
            assert_eq!(result[0].result, expected);
        }
    }

    // Task 8: `Expression::Set` applies a session RPC override and never
    // pushes a `QueryResult`. `Chain::Moonbeam` is used here rather than
    // `Chain::Ethereum` (every other test in this file resolves real
    // queries against Ethereum) precisely because `Config::set_session_rpc`
    // is process-wide global state (see its doc comment in `config.rs`):
    // overriding Ethereum's RPC here would silently redirect every other
    // test in this binary that expects the real Ethereum endpoint.

    #[tokio::test]
    async fn test_set_expression_overrides_session_rpc_without_a_query_result() {
        use crate::common::{
            config::{Config, SessionRpcTestGuard},
            types::SetRpcExpression,
        };
        use alloy::transports::http::reqwest::Url;

        let chain = Chain::Moonbeam;
        let first = Url::parse("https://first-node:8545").unwrap();
        let second = Url::parse("https://second-node:8545").unwrap();

        // `ExecutionEngine::run` calls `Config::set_session_rpc` itself, so
        // this test can't go through `SessionRpcTestGuard::acquire` — it
        // reserves the claim (panicking loudly if `Moonbeam` is already
        // claimed, or isn't a reserved test chain) and clears the override
        // on drop, same as every other session-RPC test. See
        // `SessionRpcTestGuard`'s doc comment in `config.rs`.
        let _guard = SessionRpcTestGuard::reserve(chain.clone());

        let execution_engine = ExecutionEngine::new();
        // Two `SET`s for the same chain in one program: last-write-wins,
        // not an error (see `config.rs`'s
        // `set_session_rpc_overwrites_a_previous_override_for_the_same_chain`).
        let expressions = vec![
            Expression::Set(SetRpcExpression {
                chain: chain.clone(),
                url: first,
            }),
            Expression::Set(SetRpcExpression {
                chain: chain.clone(),
                url: second.clone(),
            }),
        ];

        let results = execution_engine.run(expressions).await.unwrap();

        assert!(results.is_empty(), "SET must not push a QueryResult");
        assert_eq!(Config::session_rpc(&chain), Some(second));
    }
}
