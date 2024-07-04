use crate::common::types::Expression;

pub struct PayloadBuilder<'a> {
    expressions: &'a Vec<Expression>,
}

impl PayloadBuilder<'_> {
    fn new(expressions: &Vec<Expression>) -> PayloadBuilder {
        PayloadBuilder { expressions }
    }
}
