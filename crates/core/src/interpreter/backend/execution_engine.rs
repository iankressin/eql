use crate::common::{
    entity::Entity,
    query_result::{AccountQueryRes, BlockQueryRes, TransactionQueryRes},
    types::{AccountField, BlockField, Expression, GetExpression, TransactionField},
};
use alloy::{
    eips::BlockNumberOrTag,
    primitives::{Address, FixedBytes},
    providers::{Provider, ProviderBuilder, RootProvider},
    transports::http::{Client, Http},
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use tabled::Tabled;

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
    Block(BlockQueryRes),
    #[serde(rename = "transaction")]
    Transaction(TransactionQueryRes),
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
                let block_number = expr.entity_id.to_block_number();
                let fields = expr
                    .fields
                    .iter()
                    .map(|field| field.try_into())
                    .collect::<Result<Vec<BlockField>, _>>()?;

                if let Ok(block_number) = block_number {
                    let result = self.get_block(block_number, fields, &provider).await?;

                    Ok(ExpressionResult::Block(result))
                } else {
                    panic!("Invalid block number");
                }
            }
            Entity::Account => {
                let address = expr.entity_id.to_address().await;
                let fields = expr
                    .fields
                    .iter()
                    .map(|field| field.try_into())
                    .collect::<Result<Vec<AccountField>, _>>()?;

                if let Ok(address) = address {
                    let account = self.get_account(address, fields, &provider).await?;

                    Ok(ExpressionResult::Account(account))
                } else {
                    panic!("Invalid address");
                }
            }
            Entity::Transaction => {
                let hash = expr.entity_id.to_tx_hash();
                let fields = expr
                    .fields
                    .iter()
                    .map(|field| field.try_into())
                    .collect::<Result<Vec<TransactionField>, _>>()?;

                if let Ok(hash) = hash {
                    let tx = self.get_transaction(hash, fields, &provider).await?;

                    Ok(ExpressionResult::Transaction(tx))
                } else {
                    panic!("Invalid transaction hash");
                }
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

    async fn get_block(
        &self,
        block_id: BlockNumberOrTag,
        fields: Vec<BlockField>,
        provider: &RootProvider<Http<Client>>,
    ) -> Result<BlockQueryRes, Box<dyn Error>> {
        let mut result = BlockQueryRes::default();

        match provider.get_block_by_number(block_id, false).await? {
            Some(block) => {
                for field in &fields {
                    match field {
                        BlockField::Timestamp => {
                            result.timestamp = Some(block.header.timestamp);
                        }
                        BlockField::Number => {
                            result.number = block.header.number;
                        }
                        BlockField::Hash => {
                            result.hash = block.header.hash;
                        }
                        BlockField::ParentHash => {
                            result.parent_hash = Some(block.header.parent_hash);
                        }
                        BlockField::Size => {
                            result.size = block.size;
                        }
                        BlockField::StateRoot => {
                            result.state_root = Some(block.header.state_root);
                        }
                        BlockField::TransactionsRoot => {
                            result.transactions_root = Some(block.header.transactions_root);
                        }
                        BlockField::ReceiptsRoot => {
                            result.receipts_root = Some(block.header.receipts_root);
                        }
                        BlockField::LogsBloom => {
                            result.logs_bloom = Some(block.header.logs_bloom);
                        }
                        BlockField::ExtraData => {
                            result.extra_data = Some(block.header.extra_data.clone());
                        }
                        BlockField::MixHash => {
                            result.mix_hash = block.header.mix_hash;
                        }
                        BlockField::TotalDifficulty => {
                            result.total_difficulty = block.header.total_difficulty;
                        }
                        BlockField::BaseFeePerGas => {
                            result.base_fee_per_gas = block.header.base_fee_per_gas;
                        }
                        BlockField::WithdrawalsRoot => {
                            result.withdrawals_root = block.header.withdrawals_root;
                        }
                        BlockField::BlobGasUsed => {
                            result.blob_gas_used = block.header.blob_gas_used;
                        }
                        BlockField::ExcessBlobGas => {
                            result.excess_blob_gas = block.header.excess_blob_gas;
                        }
                        BlockField::ParentBeaconBlockRoot => {
                            result.parent_beacon_block_root = block.header.parent_beacon_block_root;
                        }
                    }
                }
            }
            // TODO: handle error
            None => panic!("Block not found"),
        }

        Ok(result)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::common::{
        chain::Chain,
        ens::NameOrAddress,
        entity_id::EntityId,
        query_result::BlockQueryRes,
        types::{AccountField, BlockField, Expression, Field, GetExpression},
    };
    use alloy::primitives::{address, b256, bloom, bytes, Address, U256};
    #[cfg(test)]
    use pretty_assertions::assert_eq;
    use std::str::FromStr;

    #[tokio::test]
    async fn test_get_block_fields() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            chain: Chain::Ethereum,
            entity: Entity::Block,
            entity_id: EntityId::Block(1.into()),
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
        let expected = vec![ExpressionResult::Block(BlockQueryRes {
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
        })];
        let execution_result = execution_engine.run(expressions).await;

        assert!(execution_result.is_ok());

        match execution_result {
            Ok(results) => {
                assert_eq!(results[0].result, expected[0]);
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
            entity_id: EntityId::Account(NameOrAddress::Address(
                Address::from_str("0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045").unwrap(),
            )),
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
            entity_id: EntityId::Account(NameOrAddress::Name(String::from("vitalik.eth"))),
            fields: vec![Field::Account(AccountField::Balance)],
            query: String::from(""),
        })];

        let execution_result = execution_engine.run(expressions).await;

        assert!(execution_result.is_ok());

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
    async fn test_get_transaction_fields() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            chain: Chain::Ethereum,
            entity: Entity::Transaction,
            entity_id: EntityId::Transaction(b256!(
                "72546b3ca8ef0dfb85fe66d19645e44cb519858c72fbcad0e1c1699256fed890"
            )),
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
}
