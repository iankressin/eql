use crate::common::{
    entity::{Entity, EntityError},
    query_result::{AccountQueryRes, BlockQueryRes, TransactionQueryRes, LogQueryRes},
    types::{AccountField, BlockField, TransactionField, LogField, Expression, GetExpression},
};
use alloy::{
    primitives::{Address, FixedBytes},
    providers::{Provider, ProviderBuilder, RootProvider},
    transports::http::{Client, Http},
    rpc::types::Filter
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use tabled::Tabled;

use super::block_resolver::resolve_block_query;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub struct QueryResult {
    pub query: String,
    pub result: ExpressionResult,
}

impl QueryResult {
    pub fn new(query: String, result: ExpressionResult) -> QueryResult {
        QueryResult { query, result }
    }
}

#[derive(Debug, PartialEq, Eq, Tabled, Serialize, Deserialize, Clone)]
pub enum ExpressionResult {
    #[serde(rename = "account")]
    Account(AccountQueryRes),
    #[serde(rename = "block")]
    Block(Vec<BlockQueryRes>),
    #[serde(rename = "transaction")]
    Transaction(TransactionQueryRes),
    #[serde(rename = "log")]
    Log(LogQueryRes),
}

pub struct ExecutionEngine;

// TODO: create ExecutionEngineErrors instead of throwing static strings
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

        match expr.entity {
            Entity::Block => {
                // let mut start_block;
                // let mut end_block;

                //First, check if expr.entity_id is Some(entity_id). If this condition is true, it calls the to_block_id method on entity_id.
                //If expr.entity_id is None, the code then checks if expr.entity_filter is Some(entity_filter). If this condition is true, it calls the to_block_range method on entity_filter. 
                //If neither entity_id nor entity_filter is present in expr, the code panics with the message "Invalid block_id".
            //    let Some(start_block, end_block) = expr.entity_id.map(|entity_id| {
            //         let start_block = entity_id.to_block_id();
            //         let end_block = None;
            //         (start_block, end_block)});


                let (start_block, end_block) = if let Some(entity_id) = &expr.entity_id {
                    let start_block = entity_id.to_block_id()?;
                    let end_block= None;
                    (start_block, end_block)
                } else if let Some(entity_filter) = &expr.entity_filter {
                    let (start_block, end_block) = entity_filter.to_block_range()?;
                    (start_block, end_block)
                } else {
                    panic!("Invalid block_id"); 
                };
        
                
                let fields = expr
                    .fields
                    .iter()
                    .map(|field| field.try_into())
                    .collect::<Result<Vec<BlockField>, _>>()?;

                let block_query_res = resolve_block_query(start_block, end_block, fields, &provider).await?;

                Ok(ExpressionResult::Block(block_query_res))
            }
            Entity::Account => {
                let address = if let Some(address_id) = &expr.entity_id {
                    address_id.to_address().await
                } else {
                    panic!("Invalid address");
                };

                let fields = expr
                    .fields
                    .iter()
                    .map(|field| field.try_into())
                    .collect::<Result<Vec<AccountField>, _>>()?;
                match address {
                    Ok(address) => {
                        let account = self.get_account(address, fields, &provider).await?;
                        Ok(ExpressionResult::Account(account))
                    }
                    Err(err) => Err(EntityError::InvalidEntity(err.to_string()).into()),
                }
            }
            Entity::Transaction => {
                let hash = if let Some(tx_id) = &expr.entity_id {
                    tx_id.to_tx_hash()
                } else {
                    panic!("Invalid transaction hash");
                };

                let fields = expr
                    .fields
                    .iter()
                    .map(|field| field.try_into())
                    .collect::<Result<Vec<TransactionField>, _>>()?;

                match hash {
                    Ok(hash) => {
                        let tx = self.get_transaction(hash, fields, &provider).await?;
                        Ok(ExpressionResult::Transaction(tx))
                    }
                    Err(err) => Err(EntityError::InvalidEntity(err.to_string()).into()),
                }
            }
            Entity::Log => {
                let filter = if let Some(filter) = &expr.entity_filter{
                    filter.to_filter()?
                } else {
                    panic!("Invalid log filter");
                };
                let fields = expr
                    .fields
                    .iter()
                    .map(|field| field.try_into())
                    .collect::<Result<Vec<LogField>, _>>()?;

                let log = self.get_logs(filter, fields, &provider).await?;
                Ok(ExpressionResult::Log(log))
            }
            Entity::Log => {
                let filter = if let Some(filter) = &expr.entity_filter{
                    filter.to_filter()?
                } else {
                    panic!("Invalid log filter");
                };
                let fields = expr
                    .fields
                    .iter()
                    .map(|field| field.try_into())
                    .collect::<Result<Vec<LogField>, _>>()?;

                let log = self.get_logs(filter, fields, &provider).await?;
                Ok(ExpressionResult::Log(log))
            }
        }
    }

    async fn get_transaction(
        &self,
        hash: FixedBytes<32>,
        fields: Vec<TransactionField>,
        provider: &RootProvider<Http<Client>>,
    ) -> Result<TransactionQueryRes, Box<dyn Error>> {
        let mut result = TransactionQueryRes::default();
        match provider.get_transaction_by_hash(hash).await? {
            Some(tx) => {
                for field in fields {
                    match field {
                        TransactionField::TransactionType => {
                            result.transaction_type = tx.transaction_type;
                        }
                        TransactionField::Hash => {
                            result.hash = Some(tx.hash);
                        }
                        TransactionField::From => {
                            result.from = Some(tx.from);
                        }
                        TransactionField::To => {
                            result.to = tx.to;
                        }
                        TransactionField::Data => {
                            result.data = Some(tx.input.clone());
                        }
                        TransactionField::Value => {
                            result.value = Some(tx.value);
                        }
                        TransactionField::GasPrice => {
                            result.gas_price = tx.gas_price;
                        }
                        TransactionField::Gas => {
                            result.gas = Some(tx.gas);
                        }
                        TransactionField::Status => {
                            match provider.get_transaction_receipt(hash).await? {
                                Some(receipt) => {
                                    result.status = Some(receipt.status());
                                }
                                None => {
                                    result.status = None;
                                }
                            }
                        }
                        TransactionField::ChainId => {
                            result.chain_id = tx.chain_id;
                        }
                        TransactionField::V => {
                            result.v = tx.signature.map_or(None, |s| Some(s.v));
                        }
                        TransactionField::R => {
                            result.r = tx.signature.map_or(None, |s| Some(s.r));
                        }
                        TransactionField::S => {
                            result.s = tx.signature.map_or(None, |s| Some(s.s));
                        }
                        TransactionField::MaxFeePerBlobGas => {
                            result.max_fee_per_blob_gas = tx.max_fee_per_blob_gas;
                        }
                        TransactionField::MaxFeePerGas => {
                            result.max_fee_per_gas = tx.max_fee_per_gas;
                        }
                        TransactionField::MaxPriorityFeePerGas => {
                            result.max_priority_fee_per_gas = tx.max_priority_fee_per_gas;
                        }
                        TransactionField::YParity => {
                            result.y_parity = tx
                                .signature
                                .map_or(None, |s| s.y_parity)
                                .map_or(None, |y| Some(y.0));
                        }
                    }
                }
            }
            None => panic!("Transaction not found"),
        }

        Ok(result)
    }

    async fn get_account(
        &self,
        address: Address,
        fields: Vec<AccountField>,
        provider: &RootProvider<Http<Client>>,
    ) -> Result<AccountQueryRes, Box<dyn Error>> {
        let mut account = AccountQueryRes::default();

        for field in &fields {
            match field {
                AccountField::Balance => {
                    account.balance = Some(provider.get_balance(address).await?);
                }
                AccountField::Nonce => {
                    account.nonce = Some(provider.get_transaction_count(address).await?);
                }
                AccountField::Address => {
                    account.address = Some(address);
                }
                AccountField::Code => {
                    account.code = Some(provider.get_code_at(address).await?);
                }
            }
        }

        Ok(account)
    }

    async fn get_logs(
        &self,
        filter: Filter,
        fields: Vec<LogField>,
        provider: &RootProvider<Http<Client>>,
    ) -> Result<LogQueryRes, Box<dyn Error>> {

        let mut result = LogQueryRes::default();
        let log = provider.get_logs(&filter).await?;
        if log.is_empty() {
            return Err("No logs found".into()); // Check if this is the best approach for no logs return. I understand it shouldn't panic.
        }
        else{
            let log = log[0].clone(); //Fix to return a range
            println!("{:#?}", log);
            for field in &fields {
                match field {
                    LogField::Address => {
                        result.address = Some(log.inner.address);
                    }
                    LogField::Topic0 => {
                        result.topic0 = log.topic0().copied();
                    }
                    LogField::Topic1 => {
                        result.topic1 = log.inner.data.topics().get(1).copied();
                    }
                    LogField::Topic2 => {
                        result.topic2 = log.inner.data.topics().get(2).copied();
                    }
                    LogField::Topic3 => {
                        result.topic3 = log.inner.data.topics().get(3).copied();
                    }
                    LogField::Data => {
                        result.data = Some(log.data().data.clone());
                    }
                    LogField::BlockHash => {
                        result.block_hash = log.block_hash;
                    }
                    LogField::BlockNumber => {
                        result.block_number = log.block_number;
                    }
                    LogField::BlockTimestamp => {
                        result.block_timestamp = log.block_timestamp;
                    }
                    LogField::TransactionHash => {
                        result.transaction_hash = log.transaction_hash;
                    }
                    LogField::TransactionIndex => {
                        result.transaction_index = log.transaction_index;
                    }
                    LogField::LogIndex => {
                        result.log_index = log.log_index;
                    }
                    LogField::Removed => {
                        result.removed = Some(log.removed); //Check this return is ok
                    }
                }
            }
            Ok(result)
        }
        
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::common::{
        chain::Chain, ens::NameOrAddress, entity_filter::{BlockRange, EntityFilter}, entity_id::EntityId, query_result::BlockQueryRes, types::{AccountField, BlockField, Expression, Field, GetExpression}
    };
    use alloy::{
        eips::BlockNumberOrTag,
        primitives::{address, b256, bloom, bytes, Address, U256},
    };
    use pretty_assertions::assert_eq;
    use std::str::FromStr;

    #[tokio::test]
    async fn test_get_logs() {
        // let contract_address = Address::from_str("0x3c11f6265ddec22f4d049dde480615735f451646").unwrap();
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            chain: Chain::Ethereum,
            entity: Entity::Log,
            entity_id: None,
            entity_filter: Some(EntityFilter::Log(BlockRange::new(BlockNumberOrTag::Number(20526954), None))),
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
        })];
        let execution_result = execution_engine.run(expressions).await; 

        println!("{:#?}", execution_result);
    }


    #[tokio::test]
    async fn test_get_block_fields() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            chain: Chain::Ethereum,
            entity: Entity::Block,
            entity_id: Some(EntityId::Block(BlockNumberOrTag::Number(1))),
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
        })];
        let expected = ExpressionResult::Block(vec![BlockQueryRes {
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
        }]);
        let execution_result = execution_engine.run(expressions).await;

        match execution_result {
            Ok(results) => {
                assert_eq!(results[0].result, expected);
            }
            Err(_) => panic!("Error"),
        }

    }

    #[tokio::test]
    async fn test_get_account_fields() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            chain: Chain::Ethereum,
            entity: Entity::Account,
            entity_id: Some(EntityId::Account(NameOrAddress::Address(
                Address::from_str("0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045").unwrap(),
            ))),
            entity_filter: None,
            fields: vec![Field::Account(AccountField::Balance)],
            query: String::from(""),
        })];
        let execution_result = execution_engine.run(expressions).await;

        match execution_result {
            Ok(results) => match &results[0] {
                QueryResult { query, result } => {
                    assert_eq!(query, "");
                    match result {
                        ExpressionResult::Account(account) => {
                            assert!(account.balance.is_some());
                        }
                        _ => panic!("Invalid result"),
                    }
                }
            },
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_get_account_fields_using_ens() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            chain: Chain::Ethereum,
            entity: Entity::Account,
            entity_id: Some(EntityId::Account(NameOrAddress::Name(String::from("vitalik.eth")))),
            entity_filter: None,
            fields: vec![Field::Account(AccountField::Balance)],
            query: String::from(""),
        })];
        let execution_result = execution_engine.run(expressions).await;

        match &execution_result.unwrap()[0] {
            QueryResult { query, result } => {
                assert_eq!(query, "");
                match result {
                    ExpressionResult::Account(account) => {
                        assert!(account.balance.is_some());
                    }
                    _ => panic!("Invalid result"),
                }
            }
        }
    }
    #[tokio::test]
    async fn test_get_account_fields_using_invalid_ens() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            chain: Chain::Ethereum,
            entity: Entity::Account,
            entity_id: Some(EntityId::Account(NameOrAddress::Name(String::from(
                "thisisinvalid235790123801.eth",
            )))),
            entity_filter: None,
            fields: vec![Field::Account(AccountField::Balance)],
            query: String::from(""),
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
            entity_id: Some(EntityId::Transaction(b256!("72546b3ca8ef0dfb85fe66d19645e44cb519858c72fbcad0e1c1699256fed890"))),
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
        })];
        let result = execution_engine.run(expressions).await;
        let expected = vec![ExpressionResult::Transaction(TransactionQueryRes {
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
        })];

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
            entity_id: Some(EntityId::Transaction(b256!(
                "bebd3baab326f895289ecbd4210cf886ce41952316441ae4cac35f00f0e882a6"
            ))),
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
        })];
        let _result = execution_engine.run(expressions).await;
    }
}
