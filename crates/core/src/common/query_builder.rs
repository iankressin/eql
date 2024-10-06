use super::{
    chain::Chain,
    entity::Entity,
    entity_filter::EntityFilter,
    entity_id::EntityId,
    query_result::QueryResult,
    types::{ChainOrRpc, Dump, Expression, Field, GetExpression},
};
use crate::interpreter::backend::execution_engine::ExecutionEngine;
use std::error::Error;

#[derive(Debug, thiserror::Error)]
pub enum EQLBuilderError {
    #[error("Fields must be set")]
    MissingFieldsError,
    #[error("Entity must be set")]
    MissingEntityError,
    #[error("EntityId must be set")]
    MissingEntityIdError,
    #[error("Chain must be set")]
    MissingChainError,
    #[error("Builder can only execute one query at a time")]
    SingleQueryError,
}

pub struct EQLBuilder {
    fields: Option<Vec<Field>>,
    entity: Option<Entity>,
    entity_id: Option<Vec<EntityId>>,
    entity_filters: Option<Vec<EntityFilter>>,
    chain: Option<ChainOrRpc>,
    dump: Option<Dump>,
}

impl EQLBuilder {
    pub fn new() -> Self {
        EQLBuilder {
            fields: None,
            entity: None,
            entity_id: None,
            entity_filters: None,
            chain: None,
            dump: None,
        }
    }

    pub fn get(&mut self, fields: Vec<Field>) -> &mut Self {
        self.fields = Some(fields);
        self
    }

    pub fn from(&mut self, entity: Entity, entity_id: Vec<EntityId>) -> &mut Self {
        self.entity = Some(entity);
        self.entity_id = Some(entity_id);
        self
    }

    pub fn on(&mut self, chain: ChainOrRpc) -> &mut Self {
        self.chain = Some(chain);
        self
    }

    pub fn dump(&mut self, dump_file: Dump) -> &mut Self {
        self.dump = Some(dump_file);
        self
    }

    pub async fn run(&self) -> Result<QueryResult, Box<dyn Error>> {
        let expression = self.expression()?;
        let result = ExecutionEngine::new().run(vec![expression]).await?;

        match result.first() {
            Some(result) => Ok(result.clone()),
            None => Err(Box::new(EQLBuilderError::SingleQueryError)),
        }
    }

    fn expression(&self) -> Result<Expression, EQLBuilderError> {
        let fields = self
            .fields
            .clone()
            .ok_or(EQLBuilderError::MissingFieldsError)?;
        let entity = self
            .entity
            .clone()
            .ok_or(EQLBuilderError::MissingEntityError)?;
        let entity_id = self.entity_id.clone();
        let chain = self
            .chain
            .clone()
            .ok_or(EQLBuilderError::MissingChainError)?;
        let entity_filter = self.entity_filters.clone();

        Ok(Expression::Get(GetExpression {
            fields,
            entity,
            entity_id,
            entity_filter,
            chain_or_rpc: chain,
            query: "".to_string(),
            dump: None,
        }))
    }
}
