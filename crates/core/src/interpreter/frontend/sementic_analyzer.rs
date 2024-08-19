use crate::common::{
    entity::Entity,
    types::{Expression, Field, GetExpression},
};
use std::error::Error;

#[derive(Debug)]
pub enum SemanticError {
    InvalidField { field: Field, enetity: Entity },
}

impl std::fmt::Display for SemanticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SemanticError::InvalidField { field, enetity } => {
                write!(f, "Invalid field `{}` for entity `{}`", field, enetity)
            }
        }
    }
}

impl Error for SemanticError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

pub struct SemanticAnalyzer<'a> {
    expressions: &'a Vec<Expression>,
}

impl<'a> SemanticAnalyzer<'a> {
    pub fn new(expressions: &'a Vec<Expression>) -> Self {
        SemanticAnalyzer { expressions }
    }

    // TODO: fields should only contain fields that are valid for the entity
    pub fn analyze(&self) -> Result<(), Box<dyn Error>> {
        for expression in self.expressions {
            match expression {
                Expression::Get(get_expr) => {
                    self.analyze_get_expr(get_expr)?;
                }
            }
        }

        Ok(())
    }

    fn analyze_get_expr(&self, get_expr: &GetExpression) -> Result<(), Box<dyn Error>> {
        for field in &get_expr.fields {
            match field {
                Field::Block(_) => {
                    if get_expr.entity != Entity::Block {
                        return Err(Box::new(SemanticError::InvalidField {
                            field: field.clone(),
                            enetity: get_expr.entity.clone(),
                        }));
                    }
                }
                Field::Account(_) => {
                    if get_expr.entity != Entity::Account {
                        return Err(Box::new(SemanticError::InvalidField {
                            field: field.clone(),
                            enetity: get_expr.entity.clone(),
                        }));
                    }
                }
                Field::Transaction(_) => {
                    if get_expr.entity != Entity::Transaction {
                        return Err(Box::new(SemanticError::InvalidField {
                            field: field.clone(),
                            enetity: get_expr.entity.clone(),
                        }));
                    }
                }
                Field::Log(_) => {
                    if get_expr.entity != Entity::Log {
                        return Err(Box::new(SemanticError::InvalidField {
                            field: field.clone(),
                            enetity: get_expr.entity.clone(),
                        }));
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::common::{
        chain::Chain,
        entity::Entity,
        types::{BlockField, Expression, Field, GetExpression},
    };

    #[test]
    fn test_analyze_get_expression_with_wrong_fields() {
        let expressions = vec![Expression::Get(GetExpression {
            entity: Entity::Account,
            entity_id: Some("0x1234567890123456789012345678901234567890"
                .try_into()
                .unwrap()),
            entity_filter: None,
            chain: Chain::Ethereum,
            fields: vec![Field::Block(BlockField::Number)],
            // The query doesn't matter for this test
            query: String::from(""),
        })];
        let analyzer = super::SemanticAnalyzer::new(&expressions);
        match analyzer.analyze() {
            Ok(_) => panic!("Expected an error"),
            Err(error) => {
                assert_eq!(
                    error.to_string(),
                    "Invalid field `number` for entity `account`"
                );
            }
        }
    }
}
