use std::error::Error;
use alloy::providers::{ProviderBuilder, Provider};
use crate::common::types::{Entity, Expression, GetExpression};

struct ExecutionEngine;

impl ExecutionEngine {
    fn new() -> ExecutionEngine {
        ExecutionEngine
    }

    async fn run(&self, expressions: Vec<Expression>) -> Result<(), Box<dyn Error>> {
        for expression in expressions {
            match expression {
                Expression::Get(get_expr) => {
                    self.run_get_expr(&get_expr).await?;
                }
                _ => return Err("Invalid expression".into()),
            }
        }

        Ok(())
    }

    async fn run_get_expr(&self, expr: &GetExpression) -> Result<(), Box<dyn std::error::Error>> {
        let rpc_url = expr.chain.rpc_url().parse()?;
        let provider = ProviderBuilder::new().on_http(rpc_url);

        match expr.entity {
            Entity::Block => {
                let block_number = expr.entity_id.into()?;
                let block = provider.get_block_by_number(block_number, false).await?;
                println!("{:?}", block);
            }
            Entity::Account => {
                let account = provider.get_account(expr.entity_id).await?;
                println!("{:?}", account);
            }
            Entity::Transaction => {
                let hash = expr.entity_id.into()?;
                let tx = provider.get_transaction_by_hash(hash).await?;
                println!("{:?}", tx);
            }
        }

        Ok(())
    }
}
