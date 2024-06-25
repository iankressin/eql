use crate::common::types::{Expression, GetExpression, Field, Entity};

pub struct SemanticAnalyzer<'a> {
    expressions: &'a Vec<Expression>,
}

impl<'a> SemanticAnalyzer<'a> {
    pub fn new(expressions: &'a Vec<Expression>) -> Self {
        SemanticAnalyzer { expressions }
    }

    pub fn analyze(&self) -> Result<(), &'static str> {
        for expression in self.expressions {
            match expression {
                Expression::Get(get_expr) => {
                    self.analyze_get_expr(get_expr)?;
                }
                _ => return Err("Invalid expression"),
            }
        }

        Ok(())
    }

    // TODO: enhance error handling
    fn analyze_get_expr(&self, get_expr: &GetExpression) -> Result<(), &'static str> {
        for field in &get_expr.fields {
            match field {
                Field::Block(_) => {
                    if get_expr.entity != Entity::Block {
                        return Err("Invalid field for entity")
                    }
                }
                Field::Account(_) => {
                    if get_expr.entity != Entity::Account {
                        return Err("Invalid field for entity entity")
                    }
                }
                Field::Transaction(_) => {
                    if get_expr.entity != Entity::Transaction {
                        return Err("Invalid field for entity")
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
        types::{
            BlockField,
            Entity,
            Expression,
            Field,
            GetExpression
        },
    };

    #[test]
    fn test_analyze_get_expression_with_wrong_fields() {
        let expressions = vec![
            Expression::Get(GetExpression {
                entity: Entity::Account,
                entity_id: "0x1234567890123456789012345678901234567890".try_into().unwrap(),
                chain: Chain::Ethereum,
                fields: vec![Field::Block(BlockField::Number)],
            }),
        ];
        let analyzer = super::SemanticAnalyzer::new(&expressions);
        let result = analyzer.analyze();

        assert_eq!(result, Err("Invalid field for entity"));
    }
}
