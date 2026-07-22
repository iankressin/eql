//! Flattens a SQL `WHERE` clause into simple `Condition`s and pulls the
//! `chain` conditions out of that list into a `Vec<ChainOrRpc>`, leaving the
//! remaining conditions for later stages to turn into entity filters.

use super::{values::expr_as_string, EqlSqlError};
use crate::common::chain::{Chain, ChainOrRpc};
use alloy::transports::http::reqwest::Url;
use sqlparser::ast::{BinaryOperator, Expr, UnaryOperator};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum CondOp {
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    In,
    Between,
}

#[derive(Debug)]
pub struct Condition {
    pub column: String,
    pub op: CondOp,
    pub values: Vec<Expr>,
}

fn column_name(expr: &Expr) -> Result<String, EqlSqlError> {
    match expr {
        Expr::Identifier(ident) => Ok(ident.value.to_ascii_lowercase()),
        other => Err(EqlSqlError::NotSupported(format!(
            "left side of a condition must be a column name, got {other}"
        ))),
    }
}

/// Flattens `AND`-conjoined conditions from a `WHERE` clause.
///
/// Rejects `OR`, `NOT`, and any construct that isn't a simple comparison,
/// `IN`, or `BETWEEN`. Each rejection names the construct as the user wrote
/// it (via `Expr`'s `Display`), not a fixed placeholder — a `LIKE` clause is
/// reported as a `LIKE` clause, a unary minus is reported as a unary minus,
/// and so on.
pub fn split_conditions(selection: Option<&Expr>) -> Result<Vec<Condition>, EqlSqlError> {
    let mut out = Vec::new();
    if let Some(expr) = selection {
        collect(expr, &mut out)?;
    }
    Ok(out)
}

fn collect(expr: &Expr, out: &mut Vec<Condition>) -> Result<(), EqlSqlError> {
    match expr {
        Expr::BinaryOp {
            left,
            op: BinaryOperator::And,
            right,
        } => {
            collect(left, out)?;
            collect(right, out)
        }
        Expr::BinaryOp {
            op: BinaryOperator::Or,
            ..
        } => Err(EqlSqlError::NotSupported("OR".into())),
        // sqlparser 0.52 has no `Expr::Not` variant: `NOT <expr>` parses as
        // `Expr::UnaryOp { op: UnaryOperator::Not, .. }`. Only that specific
        // operator is reported as "NOT" — other unary operators (e.g. unary
        // minus) fall through to the catch-all below, which names the
        // construct the user actually wrote.
        Expr::UnaryOp {
            op: UnaryOperator::Not,
            ..
        } => Err(EqlSqlError::NotSupported("NOT".into())),
        Expr::Nested(inner) => collect(inner, out),
        Expr::BinaryOp { left, op, right } => {
            let cond_op = match op {
                BinaryOperator::Eq => CondOp::Eq,
                BinaryOperator::NotEq => CondOp::Neq,
                BinaryOperator::Gt => CondOp::Gt,
                BinaryOperator::GtEq => CondOp::Gte,
                BinaryOperator::Lt => CondOp::Lt,
                BinaryOperator::LtEq => CondOp::Lte,
                other => return Err(EqlSqlError::NotSupported(format!("operator {other}"))),
            };
            out.push(Condition {
                column: column_name(left)?,
                op: cond_op,
                values: vec![(**right).clone()],
            });
            Ok(())
        }
        Expr::InList {
            expr,
            list,
            negated,
        } => {
            if *negated {
                return Err(EqlSqlError::NotSupported("NOT IN".into()));
            }
            if list.is_empty() {
                return Err(EqlSqlError::NotSupported("empty IN (...) list".into()));
            }
            out.push(Condition {
                column: column_name(expr)?,
                op: CondOp::In,
                values: list.clone(),
            });
            Ok(())
        }
        Expr::Between {
            expr,
            negated,
            low,
            high,
        } => {
            if *negated {
                return Err(EqlSqlError::NotSupported("NOT BETWEEN".into()));
            }
            out.push(Condition {
                column: column_name(expr)?,
                op: CondOp::Between,
                values: vec![(**low).clone(), (**high).clone()],
            });
            Ok(())
        }
        other => Err(EqlSqlError::NotSupported(format!("condition {other}"))),
    }
}

/// Renders a `CondOp` the way the user wrote it, for error messages that
/// must name the actual construct rather than a fixed placeholder.
fn cond_op_text(op: CondOp) -> &'static str {
    match op {
        CondOp::Eq => "=",
        CondOp::Neq => "!=",
        CondOp::Gt => ">",
        CondOp::Gte => ">=",
        CondOp::Lt => "<",
        CondOp::Lte => "<=",
        CondOp::In => "IN",
        CondOp::Between => "BETWEEN",
    }
}

fn chain_value(expr: &Expr) -> Result<Vec<ChainOrRpc>, EqlSqlError> {
    let text = expr_as_string(expr)?;
    if text == "*" {
        return Chain::from_selector("*").map_err(|e| EqlSqlError::Validation(e.to_string()));
    }
    if text.starts_with("http://") || text.starts_with("https://") {
        let url = Url::parse(&text)
            .map_err(|e| EqlSqlError::Validation(format!("invalid RPC url '{text}': {e}")))?;
        return Ok(vec![ChainOrRpc::Rpc(url)]);
    }
    Chain::try_from(text.as_str())
        .map(|c| vec![ChainOrRpc::Chain(c)])
        .map_err(|e| EqlSqlError::Validation(e.to_string()))
}

/// Removes and returns the `chain` conditions from `conds`, leaving the rest
/// untouched for later stages to turn into entity filters.
pub fn extract_chains(conds: &mut Vec<Condition>) -> Result<Vec<ChainOrRpc>, EqlSqlError> {
    let mut chains: Vec<ChainOrRpc> = Vec::new();
    let mut kept = Vec::new();
    // Unlike `blocks.number` (`build_block` in `translate.rs`), which
    // intentionally allows repeated conditions to combine an exact match
    // with a range, `chain` has no such use for a second condition:
    // `chain = a AND chain = b` can never match (a contradiction, and
    // duplicates every row when `a == b`), so a second `chain` condition is
    // rejected rather than silently unioned. `IN (...)` is the sanctioned
    // way to match several chains.
    let mut chain_conditions = 0u32;
    for cond in conds.drain(..) {
        if cond.column == "chain" {
            chain_conditions += 1;
            if chain_conditions > 1 {
                return Err(EqlSqlError::NotSupported(
                    "chain is given more than once; use IN (...) to match several chains".into(),
                ));
            }
            match cond.op {
                CondOp::Eq | CondOp::In => {
                    for value in &cond.values {
                        chains.extend(chain_value(value)?);
                    }
                }
                other_op => {
                    return Err(EqlSqlError::NotSupported(format!(
                        "chain operator {} (only chain = ... or chain IN (...))",
                        cond_op_text(other_op)
                    )))
                }
            }
        } else {
            kept.push(cond);
        }
    }
    *conds = kept;
    if chains.is_empty() {
        return Err(EqlSqlError::Validation(
            "no target chain; add e.g. AND chain = eth, or chain = '*' for all chains".into(),
        ));
    }
    Ok(chains)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::dialect::DuckDbDialect;
    use sqlparser::parser::Parser;

    fn where_of(sql: &str) -> Option<sqlparser::ast::Expr> {
        let stmts = Parser::parse_sql(&DuckDbDialect {}, sql).unwrap();
        match stmts.into_iter().next().unwrap() {
            sqlparser::ast::Statement::Query(q) => match *q.body {
                sqlparser::ast::SetExpr::Select(s) => s.selection.clone(),
                _ => panic!("not a select"),
            },
            _ => panic!("not a query"),
        }
    }

    #[test]
    fn splits_and_conjuncts() {
        let sel = where_of("SELECT a FROM t WHERE x = 1 AND y > 2 AND z IN (1,2)");
        let conds = split_conditions(sel.as_ref()).unwrap();
        assert_eq!(conds.len(), 3);
        assert_eq!(conds[0].column, "x");
        assert_eq!(conds[0].op, CondOp::Eq);
        assert_eq!(conds[1].op, CondOp::Gt);
        assert_eq!(conds[2].op, CondOp::In);
        assert_eq!(conds[2].values.len(), 2);
    }

    #[test]
    fn between_carries_low_and_high() {
        let sel = where_of("SELECT a FROM t WHERE n BETWEEN 1 AND 100");
        let conds = split_conditions(sel.as_ref()).unwrap();
        assert_eq!(conds[0].op, CondOp::Between);
        assert_eq!(conds[0].values.len(), 2);
    }

    #[test]
    fn rejects_or_and_not() {
        let sel = where_of("SELECT a FROM t WHERE x = 1 OR y = 2");
        let err = split_conditions(sel.as_ref()).unwrap_err().to_string();
        assert!(err.contains("OR"));
        let sel = where_of("SELECT a FROM t WHERE NOT x = 1");
        assert!(split_conditions(sel.as_ref()).is_err());
    }

    #[test]
    fn extracts_chains() {
        use crate::common::chain::{Chain, ChainOrRpc};
        let sel = where_of("SELECT a FROM t WHERE chain = eth AND x = 1");
        let mut conds = split_conditions(sel.as_ref()).unwrap();
        let chains = extract_chains(&mut conds).unwrap();
        assert_eq!(chains, vec![ChainOrRpc::Chain(Chain::Ethereum)]);
        assert_eq!(conds.len(), 1); // chain condition removed

        let sel = where_of("SELECT a FROM t WHERE chain IN (eth, base)");
        let mut conds = split_conditions(sel.as_ref()).unwrap();
        assert_eq!(extract_chains(&mut conds).unwrap().len(), 2);
    }

    // Fix 1: a second, separate `chain` condition is a contradiction in SQL
    // terms (`chain = a AND chain = b` can never match, and duplicates every
    // row when `a == b`), not a union — `IN (...)` is the sanctioned way to
    // match several chains. Unlike `blocks.number` (`build_block` in
    // `translate.rs`), which intentionally allows repeated conditions to
    // combine an exact match with a range, `chain` has no such use for a
    // second condition.
    #[test]
    fn duplicate_chain_condition_is_rejected_clearly() {
        let sel = where_of("SELECT a FROM t WHERE chain = eth AND chain = eth");
        let mut conds = split_conditions(sel.as_ref()).unwrap();
        let err = extract_chains(&mut conds).unwrap_err().to_string();
        assert!(err.contains("chain") && err.contains("IN"), "{err}");
    }

    #[test]
    fn chain_wildcard_and_url() {
        let sel = where_of("SELECT a FROM t WHERE chain = '*'");
        let mut conds = split_conditions(sel.as_ref()).unwrap();
        assert!(extract_chains(&mut conds).unwrap().len() > 5);

        let sel = where_of("SELECT a FROM t WHERE chain = 'https://my-node:8545'");
        let mut conds = split_conditions(sel.as_ref()).unwrap();
        assert!(matches!(
            extract_chains(&mut conds).unwrap()[0],
            crate::common::chain::ChainOrRpc::Rpc(_)
        ));
    }

    #[test]
    fn unsupported_chain_operators_name_their_own_operator() {
        let sel = where_of("SELECT a FROM t WHERE chain > eth");
        let mut conds = split_conditions(sel.as_ref()).unwrap();
        let gt_err = extract_chains(&mut conds).unwrap_err().to_string();
        assert!(gt_err.contains('>'));

        let sel = where_of("SELECT a FROM t WHERE chain BETWEEN a AND b");
        let mut conds = split_conditions(sel.as_ref()).unwrap();
        let between_err = extract_chains(&mut conds).unwrap_err().to_string();
        assert!(between_err.contains("BETWEEN"));

        // The two messages must actually differ, naming their own operator —
        // not collapse to one fixed string regardless of what was written.
        assert_ne!(gt_err, between_err);
    }

    #[test]
    fn missing_chain_is_an_error() {
        let sel = where_of("SELECT a FROM t WHERE x = 1");
        let mut conds = split_conditions(sel.as_ref()).unwrap();
        let err = extract_chains(&mut conds).unwrap_err().to_string();
        assert!(err.contains("chain"));
    }

    // Shapes the brief's tests don't cover, checked here so they fail
    // clearly instead of being mis-parsed or panicking.

    #[test]
    fn rejects_column_on_the_right() {
        let sel = where_of("SELECT a FROM t WHERE 1 = x");
        let err = split_conditions(sel.as_ref()).unwrap_err().to_string();
        assert!(err.contains('1'));
    }

    #[test]
    fn rejects_compound_identifier() {
        let sel = where_of("SELECT a FROM t WHERE t.x = 1");
        let err = split_conditions(sel.as_ref()).unwrap_err().to_string();
        assert!(err.contains("t.x"));
    }

    #[test]
    fn rejects_is_null() {
        let sel = where_of("SELECT a FROM t WHERE x IS NULL");
        let err = split_conditions(sel.as_ref()).unwrap_err().to_string();
        assert!(err.contains("IS NULL"));
    }

    #[test]
    fn rejects_like() {
        let sel = where_of("SELECT a FROM t WHERE x LIKE '%a%'");
        let err = split_conditions(sel.as_ref()).unwrap_err().to_string();
        assert!(err.contains("LIKE"));
    }

    #[test]
    fn flattens_nested_parenthesised_and() {
        let sel = where_of("SELECT a FROM t WHERE (x = 1 AND y = 2)");
        let conds = split_conditions(sel.as_ref()).unwrap();
        assert_eq!(conds.len(), 2);
    }

    #[test]
    fn nested_or_is_still_rejected() {
        let sel = where_of("SELECT a FROM t WHERE (x = 1 OR y = 2) AND z = 3");
        let err = split_conditions(sel.as_ref()).unwrap_err().to_string();
        assert!(err.contains("OR"));
    }

    #[test]
    fn rejects_empty_in_list() {
        // `sqlparser` itself refuses to parse `x IN ()` as SQL text (it's a
        // parse error, not a valid empty list), so this exercises the
        // defensive check in `collect` directly against a hand-built AST.
        use sqlparser::ast::Ident;
        let expr = Expr::InList {
            expr: Box::new(Expr::Identifier(Ident::new("x"))),
            list: vec![],
            negated: false,
        };
        assert!(split_conditions(Some(&expr)).is_err());
    }

    #[test]
    fn unary_minus_is_not_reported_as_not() {
        // A standalone unary-minus conjunct isn't valid EQL, but it must be
        // rejected by its own name, not misreported as "NOT" — only
        // `UnaryOperator::Not` gets that label.
        let sel = where_of("SELECT a FROM t WHERE -x");
        let err = split_conditions(sel.as_ref()).unwrap_err().to_string();
        assert!(!err.contains("NOT"));
    }
}
