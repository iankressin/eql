use super::{
    chain::Chain,
    entity::Entity,
    entity_id::EntityId,
    query_result::QueryResult,
    types::{Expression, Field, GetExpression},
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
    entity_id: Option<EntityId>,
    chain: Option<Chain>,
}

impl EQLBuilder {
    pub fn new() -> Self {
        EQLBuilder {
            fields: None,
            entity: None,
            entity_id: None,
            chain: None,
        }
    }

    pub fn get(&mut self, fields: Vec<Field>) -> &mut Self {
        self.fields = Some(fields);
        self
    }

    pub fn from(&mut self, entity: Entity, entity_id: EntityId) -> &mut Self {
        self.entity = Some(entity);
        self.entity_id = Some(entity_id);
        self
    }

    pub fn on(&mut self, chain: Chain) -> &mut Self {
        self.chain = Some(chain);
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
        let entity_id = self
            .entity_id
            .clone()
            .ok_or(EQLBuilderError::MissingEntityIdError)?;
        let chain = self
            .chain
            .clone()
            .ok_or(EQLBuilderError::MissingChainError)?;

        Ok(Expression::Get(GetExpression {
            fields,
            entity,
            entity_id,
            chain,
            query: "".to_string(),
            dump: None,
        }))
    }
}
