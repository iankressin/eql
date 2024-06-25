use crate::common::types::{Expression, Field, GetExpression};
use pest_derive::Parser as DeriveParser;
use pest::Parser as PestParser;
use pest::iterators::Pairs;

#[derive(DeriveParser)]
#[grammar = "src/interpreter/frontend/productions.pest"]
pub struct Parser<'a> {
    source: &'a str
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Self {
        Parser { source }
    }

    pub fn parse_expressions(&self) -> Result<Vec<Expression>, &'static str> {
        let mut expressions: Vec<Expression> = vec![];
        let pairs = Parser::parse(Rule::program, self.source)
            .unwrap_or_else(|e| panic!("{}", e));

        for pair in pairs {
            match pair.as_rule() {
                Rule::get => {
                    let inner_pair = pair.clone().into_inner();
                    let get_expr = self.parse_get_expr(inner_pair)?;

                    expressions.push(Expression::Get(get_expr));
                },
                unexpected_token => panic!("Unexpected token: {:?}", unexpected_token),
            }
        }

        Ok(expressions)
    }

    fn parse_get_expr(&self, pairs: Pairs<Rule>) -> Result<GetExpression, &'static str> {
        let mut current_pair = pairs;
        let mut get_expr = GetExpression::default();

        while let Some(pair) = current_pair.next() {
            match pair.as_rule() {
                Rule::fields => {
                    let inner_pair = pair.clone().into_inner();
                    get_expr.fields = self.get_fields(inner_pair)?;
                },
                Rule::entity => get_expr.entity = pair.as_str().try_into()?,
                Rule::entity_id => get_expr.entity_id = pair.as_str().try_into()?,
                Rule::chain => get_expr.chain = pair.as_str().try_into()?,
                _ => return Err("Unexpected token"),
            }
        }

        Ok(get_expr)
    }

    fn get_fields(&self, pairs: Pairs<Rule>) -> Result<Vec<Field>, &'static str> {
        let mut fields: Vec<Field> = Vec::new();

        for pair in pairs {
            match pair.as_rule() {
                Rule::account_field | Rule::block_field | Rule::tx_field => {
                    fields.push(pair.as_str().try_into()?);
                },
                _ => return Err("Unexpected token"),
            }
        }

        Ok(fields)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use alloy::primitives::Address;
    use crate::common::{
        chain::Chain,
        types::*,
    };

    #[test]
    fn test_build_get_ast() {
        let source = "GET nonce, balance FROM account 0x1234567890123456789012345678901234567890 ON ethereum";
        let parser = Parser::new(source);
        let address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
        let result = parser.parse_expressions().unwrap();
        let expected = vec![
            Expression::Get(GetExpression {
                entity: Entity::Account,
                entity_id: EntityId::Account(address),
                fields: vec![
                    Field::Account(AccountField::Nonce),
                    Field::Account(AccountField::Balance)
                ],
                chain: Chain::Ethereum,
            })
        ];

        assert_eq!(result, expected);
    }
}

