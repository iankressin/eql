use alloy::{
    eips::BlockNumberOrTag,
    primitives::{Address, FixedBytes},
    providers::{Provider, ProviderBuilder, RootProvider},
    transports::http::{Client, Http},
};
use eql_common::types::{
    AccountField, AccountQueryRes, BlockField, BlockQueryRes, Entity, Expression, GetExpression,
    TransactionField, TransactionQueryRes,
};
use std::error::Error;
use tabled::Tabled;

#[derive(Debug, PartialEq, Eq)]
pub struct QueryResult {
    pub query: String,
    pub result: ExpressionResult,
}

impl QueryResult {
    pub fn new(query: String, result: ExpressionResult) -> QueryResult {
        QueryResult { query, result }
    }
}

#[derive(Debug, PartialEq, Eq, Tabled)]
pub enum ExpressionResult {
    Account(AccountQueryRes),
    Block(BlockQueryRes),
    Transaction(TransactionQueryRes),
}

pub struct ExecutionEngine;

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
                    panic!("2. Invalid block number");
                }
            }
            Entity::Account => {
                let address = expr.entity_id.to_address();
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
                        TransactionField::From => {
                            result.from = Some(tx.from);
                        }
                        TransactionField::To => {
                            result.to = tx.to;
                        }
                        TransactionField::Value => {
                            result.value = Some(tx.value);
                        }
                        TransactionField::GasPrice => {
                            result.gas_price = tx.gas_price;
                        }
                        TransactionField::Data => {
                            result.data = Some(tx.input.clone());
                        }
                        TransactionField::Hash => {
                            result.hash = Some(tx.hash);
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
                    let balance = provider.get_balance(address).await?;
                    account.balance = Some(balance);
                }
                AccountField::Nonce => {
                    let nonce = provider.get_transaction_count(address).await?;
                    account.nonce = Some(nonce);
                }
                AccountField::Address => {
                    account.address = Some(address);
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
    use alloy::primitives::Address;
    use eql_common::{
        chain::Chain,
        types::{
            AccountField, BlockField, BlockQueryRes, Entity, EntityId, Expression, Field,
            GetExpression,
        },
    };
    use std::str::FromStr;

    #[tokio::test]
    async fn test_get_block_fields() {
        let execution_engine = ExecutionEngine::new();
        let expressions = vec![Expression::Get(GetExpression {
            chain: Chain::Ethereum,
            entity: Entity::Block,
            entity_id: EntityId::Block(1.into()),
            fields: vec![Field::Block(BlockField::Timestamp)],
            query: String::from(""),
        })];
        let expected = vec![ExpressionResult::Block(BlockQueryRes {
            timestamp: Some(1438269988),
            number: None,
            hash: None,
            parent_hash: None,
            size: None,
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
            entity_id: EntityId::Account(
                Address::from_str("0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045").unwrap(),
            ),
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
            entity_id: EntityId::Transaction(
                FixedBytes::from_str(
                    "0x72546b3ca8ef0dfb85fe66d19645e44cb519858c72fbcad0e1c1699256fed890",
                )
                .unwrap(),
            ),
            fields: vec![Field::Transaction(TransactionField::To)],
            query: String::from(""),
        })];

        let execution_result = execution_engine.run(expressions).await;

        assert!(execution_result.is_ok());

        match &execution_result.unwrap()[0] {
            QueryResult { query, result } => {
                assert_eq!(query, "");
                match result {
                    ExpressionResult::Transaction(tx) => {
                        assert!(tx.to.is_some());
                    }
                    _ => panic!("Invalid result"),
                }
            }
        }
    }
}
