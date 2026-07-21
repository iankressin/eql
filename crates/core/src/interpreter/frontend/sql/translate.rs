//! Translates a parsed SQL `Statement` into the existing `Expression` /
//! `GetExpression` / `Entity` structs the backend already executes.
//!
//! Covers `accounts` and `blocks`; `transactions` and `logs` are Task 7's
//! job (see the `build_transaction` / `build_logs` stubs below). This module
//! also owns every statement-level rejection that `where_clause` can't see.
//!
//! `query_to_get` and `validate_select_shape` destructure `sqlparser`'s
//! `Query` and `Select` structs field-by-field, with no `..` catch-all. Each
//! field is either handled elsewhere in this function and bound with `_`
//! (with a comment saying where), or checked and rejected by name if it
//! carries meaning we'd otherwise silently drop. This is deliberate: an
//! allow-list of `if` checks lets a newly-added `sqlparser` field slip
//! through unnoticed, silently discarding whatever the user wrote; an
//! exhaustive destructure instead fails to *compile* the day `sqlparser`
//! adds a field neither arm accounts for, so the gap can't silently regrow.
//! Every field in both structs implements `Display` in a way that already
//! renders the exact keyword text the user wrote (e.g. `Top`'s `Display` is
//! `"TOP 5"`, `With`'s is `"WITH cte AS (...)"`), so the rejection messages
//! below are built from that `Display` output rather than fixed strings —
//! naming the actual construct, not a generic placeholder.

use super::{
    schema::{self, EntityKind},
    values,
    where_clause::{self, CondOp, Condition},
    EqlSqlError,
};
use crate::common::{
    account::{Account, AccountField},
    block::{Block, BlockField, BlockId, BlockRange},
    entity::Entity,
    types::{Expression, GetExpression},
};
use sqlparser::ast::{Expr, Select, SelectItem, SetExpr, Statement, TableFactor};
use std::collections::HashMap;
use std::fmt::Display;

/// Renders a slice of `Display`-able AST nodes the way the user wrote them,
/// comma-separated — used for the `Vec<_>`-typed `Query`/`Select` fields
/// (`locks`, `cluster_by`, `named_window`, ...) where sqlparser doesn't
/// provide a `Display` for the whole collection, only for each element.
fn joined<T: Display>(items: &[T]) -> String {
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn statement_to_expression(stmt: &Statement) -> Result<Expression, EqlSqlError> {
    match stmt {
        Statement::Query(query) => query_to_get(query, None),
        other => Err(EqlSqlError::NotSupported(format!("statement {other}"))),
    }
}

pub(super) fn query_to_get(
    query: &sqlparser::ast::Query,
    dump: Option<crate::common::dump::Dump>,
) -> Result<Expression, EqlSqlError> {
    // Exhaustive destructure — see the module doc comment.
    let sqlparser::ast::Query {
        with,
        body,
        order_by,
        limit,
        limit_by,
        offset,
        fetch,
        locks,
        for_clause,
        settings,
        format_clause,
    } = query;

    if let Some(with) = with {
        return Err(EqlSqlError::NotSupported(format!("{with}")));
    }
    if order_by.is_some() {
        return Err(EqlSqlError::NotSupported("ORDER BY".into()));
    }
    if offset.is_some() {
        return Err(EqlSqlError::NotSupported("OFFSET".into()));
    }
    // ClickHouse/GenericDialect-only syntax (`LIMIT n BY expr, ...`); the
    // parser never populates this under `DuckDbDialect`, but it's rejected
    // by name rather than silently ignored in case that ever changes.
    if !limit_by.is_empty() {
        return Err(EqlSqlError::NotSupported(format!(
            "LIMIT ... BY {}",
            joined(limit_by)
        )));
    }
    if let Some(fetch) = fetch {
        return Err(EqlSqlError::NotSupported(format!("{fetch}")));
    }
    if !locks.is_empty() {
        return Err(EqlSqlError::NotSupported(format!(
            "locking clause ({})",
            joined(locks)
        )));
    }
    if let Some(for_clause) = for_clause {
        return Err(EqlSqlError::NotSupported(format!("{for_clause}")));
    }
    // ClickHouse/GenericDialect-only syntax; unreachable under
    // `DuckDbDialect` today (see the module doc comment for why we still
    // check it).
    if let Some(settings) = settings {
        return Err(EqlSqlError::NotSupported(format!(
            "SETTINGS {}",
            joined(settings)
        )));
    }
    // ClickHouse/GenericDialect-only syntax; unreachable under
    // `DuckDbDialect` today.
    if let Some(format_clause) = format_clause {
        return Err(EqlSqlError::NotSupported(format!("{format_clause}")));
    }

    let limit = match limit {
        None => None,
        Some(expr) => {
            let n = values::parse_u64(expr)?;
            Some(
                usize::try_from(n)
                    .map_err(|e| EqlSqlError::Validation(format!("LIMIT {n} does not fit: {e}")))?,
            )
        }
    };
    let select = match &**body {
        SetExpr::Select(select) => select,
        other => return Err(EqlSqlError::NotSupported(format!("query form {other}"))),
    };
    validate_select_shape(select)?;

    let entity_name = table_name(select)?;
    let kind = schema::resolve_entity(&entity_name)?;
    let (field_names, aliases) = projection(select)?;

    let mut conds = where_clause::split_conditions(select.selection.as_ref())?;
    let chains = where_clause::extract_chains(&mut conds)?;

    let entity = match kind {
        EntityKind::Accounts => build_account(&field_names, conds)?,
        EntityKind::Blocks => build_block(&field_names, conds)?,
        EntityKind::Transactions => build_transaction(&field_names, conds)?,
        EntityKind::Logs => build_logs(&field_names, conds)?,
    };

    Ok(Expression::Get(GetExpression {
        entity,
        chains,
        dump,
        limit,
        aliases: if aliases.is_empty() {
            None
        } else {
            Some(aliases)
        },
    }))
}

fn validate_select_shape(select: &Select) -> Result<(), EqlSqlError> {
    // Exhaustive destructure — see the module doc comment. Fields handled
    // elsewhere in `query_to_get`/`validate_select_shape` (or that are only
    // ever meaningful alongside a field we already reject) are bound to `_`
    // with a comment, never silently dropped via `..`.
    let Select {
        distinct,
        top,
        top_before_distinct: _, // only meaningful when `top` is Some, rejected below
        projection: _,          // read by `projection()`, called separately
        into,
        from,
        lateral_views,
        prewhere,
        selection: _, // read by `where_clause`, called separately
        group_by,
        cluster_by,
        distribute_by,
        sort_by,
        having,
        named_window,
        qualify,
        window_before_qualify: _, // only meaningful alongside `named_window`/`qualify`, rejected below
        value_table_mode,
        connect_by,
    } = select;

    if let Some(distinct) = distinct {
        return Err(EqlSqlError::NotSupported(format!("{distinct}")));
    }
    if let Some(top) = top {
        return Err(EqlSqlError::NotSupported(format!("{top}")));
    }
    if let Some(into) = into {
        return Err(EqlSqlError::NotSupported(format!("{into}")));
    }
    if from.len() != 1 {
        return Err(EqlSqlError::NotSupported(
            "multiple tables in FROM (JOIN)".into(),
        ));
    }
    if !from[0].joins.is_empty() {
        return Err(EqlSqlError::NotSupported("JOIN".into()));
    }
    if !lateral_views.is_empty() {
        return Err(EqlSqlError::NotSupported(
            joined(lateral_views).trim().to_string(),
        ));
    }
    // ClickHouse/GenericDialect-only syntax; unreachable under
    // `DuckDbDialect` today.
    if let Some(prewhere) = prewhere {
        return Err(EqlSqlError::NotSupported(format!("PREWHERE {prewhere}")));
    }
    // group_by is GroupByExpr::Expressions(vec, _) when empty in 0.52 (the
    // parser defaults to it when no GROUP BY clause is present at all).
    match group_by {
        sqlparser::ast::GroupByExpr::Expressions(exprs, _) if exprs.is_empty() => {}
        other => return Err(EqlSqlError::NotSupported(format!("{other}"))),
    }
    if !cluster_by.is_empty() {
        return Err(EqlSqlError::NotSupported(format!(
            "CLUSTER BY {}",
            joined(cluster_by)
        )));
    }
    if !distribute_by.is_empty() {
        return Err(EqlSqlError::NotSupported(format!(
            "DISTRIBUTE BY {}",
            joined(distribute_by)
        )));
    }
    if !sort_by.is_empty() {
        return Err(EqlSqlError::NotSupported(format!(
            "SORT BY {}",
            joined(sort_by)
        )));
    }
    if let Some(having) = having {
        return Err(EqlSqlError::NotSupported(format!("HAVING {having}")));
    }
    if !named_window.is_empty() {
        return Err(EqlSqlError::NotSupported(format!(
            "WINDOW {}",
            joined(named_window)
        )));
    }
    if let Some(qualify) = qualify {
        return Err(EqlSqlError::NotSupported(format!("QUALIFY {qualify}")));
    }
    // BigQueryDialect-only syntax (`SELECT AS STRUCT`/`SELECT AS VALUE`);
    // unreachable under `DuckDbDialect` today.
    if let Some(value_table_mode) = value_table_mode {
        return Err(EqlSqlError::NotSupported(format!(
            "SELECT {value_table_mode}"
        )));
    }
    // Requires `Dialect::supports_connect_by()`, which `DuckDbDialect` does
    // not implement; unreachable today.
    if let Some(connect_by) = connect_by {
        return Err(EqlSqlError::NotSupported(format!("{connect_by}")));
    }
    Ok(())
}

fn table_name(select: &Select) -> Result<String, EqlSqlError> {
    match &select.from[0].relation {
        TableFactor::Table { name, .. } => Ok(name
            .0
            .iter()
            .map(|ident| ident.to_string())
            .collect::<Vec<_>>()
            .join(".")
            .to_ascii_lowercase()),
        other => Err(EqlSqlError::NotSupported(format!("FROM {other}"))),
    }
}

/// Returns (field names in canonical spelling or ["*"], alias map keyed by canonical field name).
fn projection(select: &Select) -> Result<(Vec<String>, HashMap<String, String>), EqlSqlError> {
    let mut names = Vec::new();
    let mut aliases = HashMap::new();
    for item in &select.projection {
        match item {
            SelectItem::Wildcard(_) => names.push("*".to_string()),
            SelectItem::UnnamedExpr(Expr::Identifier(ident)) => {
                names.push(ident.value.to_ascii_lowercase());
            }
            SelectItem::ExprWithAlias {
                expr: Expr::Identifier(ident),
                alias,
            } => {
                let name = ident.value.to_ascii_lowercase();
                aliases.insert(name.clone(), alias.value.clone());
                names.push(name);
            }
            other => {
                return Err(EqlSqlError::NotSupported(format!(
                    "SELECT expression '{other}' (only plain fields, * and AS)"
                )))
            }
        }
    }
    Ok((names, aliases))
}

fn build_account(fields: &[String], conds: Vec<Condition>) -> Result<Entity, EqlSqlError> {
    let fields = if fields == ["*"] {
        AccountField::all_variants().to_vec()
    } else {
        fields
            .iter()
            .map(|f| schema::resolve_account_field(f))
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut ids = Vec::new();
    for cond in conds {
        match (cond.column.as_str(), cond.op) {
            ("address", CondOp::Eq) | ("address", CondOp::In) => {
                for value in &cond.values {
                    ids.push(values::parse_name_or_address(value)?);
                }
            }
            (col, _) => {
                return Err(EqlSqlError::NotSupported(format!(
                    "filter on accounts.{col} (only address = / IN)"
                )))
            }
        }
    }
    if ids.is_empty() {
        return Err(EqlSqlError::Validation(
            "accounts queries need an address predicate (= or IN)".into(),
        ));
    }
    Ok(Entity::Account(Account::new(Some(ids), None, fields)))
}

fn build_block(fields: &[String], conds: Vec<Condition>) -> Result<Entity, EqlSqlError> {
    let fields = if fields == ["*"] {
        BlockField::all_variants().to_vec()
    } else {
        fields
            .iter()
            .map(|f| schema::resolve_block_field(f))
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut ids = Vec::new();
    for cond in conds {
        match (cond.column.as_str(), cond.op) {
            ("number", CondOp::Eq) | ("number", CondOp::In) => {
                for value in &cond.values {
                    ids.push(BlockId::Number(values::parse_block_number_or_tag(value)?));
                }
            }
            ("number", CondOp::Between) => {
                ids.push(BlockId::Range(BlockRange::new(
                    values::parse_block_number_or_tag(&cond.values[0])?,
                    Some(values::parse_block_number_or_tag(&cond.values[1])?),
                )));
            }
            (col, _) => {
                return Err(EqlSqlError::NotSupported(format!(
                    "filter on blocks.{col} (only number =, IN, BETWEEN)"
                )))
            }
        }
    }
    if ids.is_empty() {
        return Err(EqlSqlError::Validation(
            "blocks queries need a number predicate (=, IN or BETWEEN)".into(),
        ));
    }
    Ok(Entity::Block(Block::new(Some(ids), None, fields)))
}

fn build_transaction(_fields: &[String], _conds: Vec<Condition>) -> Result<Entity, EqlSqlError> {
    Err(EqlSqlError::NotSupported("transactions (Task 7)".into()))
}

fn build_logs(_fields: &[String], _conds: Vec<Condition>) -> Result<Entity, EqlSqlError> {
    Err(EqlSqlError::NotSupported("logs (Task 7)".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::{
        account::{Account, AccountField},
        block::{BlockId, BlockRange},
        chain::{Chain, ChainOrRpc},
        ens::NameOrAddress,
        types::Expression,
    };
    use alloy::eips::BlockNumberOrTag;

    fn translate_one(sql: &str) -> Result<Expression, EqlSqlError> {
        let prelexed = crate::interpreter::frontend::sql::prelex::prelex(sql)?;
        let stmts =
            sqlparser::parser::Parser::parse_sql(&sqlparser::dialect::DuckDbDialect {}, &prelexed)
                .map_err(|e| EqlSqlError::Parse(e.to_string()))?;
        statement_to_expression(&stmts[0])
    }

    #[test]
    fn account_query_translates() {
        let expr = translate_one(
            "SELECT nonce, balance FROM accounts WHERE address = vitalik.eth AND chain = eth",
        )
        .unwrap();
        let Expression::Get(get) = expr;
        assert_eq!(get.chains, vec![ChainOrRpc::Chain(Chain::Ethereum)]);
        assert_eq!(
            get.entity,
            crate::common::entity::Entity::Account(Account::new(
                Some(vec![NameOrAddress::Name("vitalik.eth".into())]),
                None,
                vec![AccountField::Nonce, AccountField::Balance],
            ))
        );
    }

    #[test]
    fn account_in_list_and_wildcard_fields() {
        let expr = translate_one(
            "SELECT * FROM accounts WHERE address IN (0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045, ian.eth) AND chain = eth",
        )
        .unwrap();
        let Expression::Get(get) = expr;
        let crate::common::entity::Entity::Account(account) = get.entity else {
            panic!()
        };
        assert_eq!(account.ids().unwrap().len(), 2);
        assert_eq!(account.fields(), AccountField::all_variants().to_vec());
    }

    #[test]
    fn block_number_eq_between_and_limit() {
        let expr = translate_one(
            "SELECT hash FROM blocks WHERE number BETWEEN 1 AND 100 AND chain = eth LIMIT 5",
        )
        .unwrap();
        let Expression::Get(get) = expr;
        assert_eq!(get.limit, Some(5));
        let crate::common::entity::Entity::Block(block) = get.entity else {
            panic!()
        };
        assert_eq!(
            block.ids().unwrap(),
            &vec![BlockId::Range(BlockRange::new(
                BlockNumberOrTag::Number(1),
                Some(BlockNumberOrTag::Number(100)),
            ))]
        );
    }

    #[test]
    fn block_latest_tag() {
        let expr =
            translate_one("SELECT * FROM blocks WHERE number = latest AND chain = eth").unwrap();
        let Expression::Get(get) = expr;
        let crate::common::entity::Entity::Block(block) = get.entity else {
            panic!()
        };
        assert_eq!(
            block.ids().unwrap(),
            &vec![BlockId::Number(BlockNumberOrTag::Latest)]
        );
    }

    #[test]
    fn aliases_are_captured() {
        let expr = translate_one(
            "SELECT balance AS eth_balance FROM accounts WHERE address = ian.eth AND chain = eth",
        )
        .unwrap();
        let Expression::Get(get) = expr;
        assert_eq!(get.aliases.unwrap().get("balance").unwrap(), "eth_balance");
    }

    #[test]
    fn rejects_unsupported_sql() {
        for (sql, needle) in [
            (
                "SELECT a FROM accounts JOIN blocks ON true WHERE chain = eth",
                "JOIN",
            ),
            (
                "SELECT count(*) FROM blocks WHERE number = 1 AND chain = eth",
                "expression",
            ),
            (
                "SELECT a FROM blocks WHERE number = 1 AND chain = eth ORDER BY a",
                "ORDER BY",
            ),
            (
                "SELECT DISTINCT a FROM blocks WHERE number = 1 AND chain = eth",
                "DISTINCT",
            ),
            (
                "SELECT a FROM blocks WHERE number = 1 AND chain = eth GROUP BY a",
                "GROUP BY",
            ),
        ] {
            let err = translate_one(sql).unwrap_err().to_string();
            assert!(err.contains(needle), "{sql} → {err}");
        }
    }

    #[test]
    fn missing_key_predicate_errors() {
        let err = translate_one("SELECT nonce FROM accounts WHERE chain = eth")
            .unwrap_err()
            .to_string();
        assert!(err.contains("address"));
        let err = translate_one("SELECT hash FROM blocks WHERE chain = eth")
            .unwrap_err()
            .to_string();
        assert!(err.contains("number"));
    }

    // Shapes the golden tests above don't cover. Each is either rejected
    // clearly (naming the real construct) or translated sensibly — never
    // mis-translated silently and never a panic.

    #[test]
    fn wildcard_alongside_named_field_is_rejected_clearly() {
        let err =
            translate_one("SELECT *, nonce FROM accounts WHERE address = ian.eth AND chain = eth")
                .unwrap_err()
                .to_string();
        assert!(err.contains('*'), "{err}");
    }

    #[test]
    fn duplicate_field_selection_translates_without_panic() {
        let expr = translate_one(
            "SELECT nonce, nonce FROM accounts WHERE address = ian.eth AND chain = eth",
        )
        .unwrap();
        let Expression::Get(get) = expr;
        let crate::common::entity::Entity::Account(account) = get.entity else {
            panic!()
        };
        assert_eq!(
            account.fields(),
            vec![AccountField::Nonce, AccountField::Nonce]
        );
    }

    #[test]
    fn limit_zero_is_accepted_as_a_valid_limit() {
        let expr = translate_one(
            "SELECT nonce FROM accounts WHERE address = ian.eth AND chain = eth LIMIT 0",
        )
        .unwrap();
        let Expression::Get(get) = expr;
        assert_eq!(get.limit, Some(0));
    }

    #[test]
    fn negative_limit_is_rejected_clearly() {
        let err = translate_one(
            "SELECT nonce FROM accounts WHERE address = ian.eth AND chain = eth LIMIT -1",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains('-') && err.contains('1'), "{err}");
    }

    #[test]
    fn huge_limit_is_rejected_clearly() {
        let err = translate_one(
            "SELECT nonce FROM accounts WHERE address = ian.eth AND chain = eth LIMIT 99999999999999999999999999",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("99999999999999999999999999"), "{err}");
    }

    #[test]
    fn chain_in_select_list_is_a_valid_output_field() {
        let expr = translate_one(
            "SELECT chain, nonce FROM accounts WHERE address = ian.eth AND chain = eth",
        )
        .unwrap();
        let Expression::Get(get) = expr;
        let crate::common::entity::Entity::Account(account) = get.entity else {
            panic!()
        };
        assert_eq!(
            account.fields(),
            vec![AccountField::Chain, AccountField::Nonce]
        );
    }

    #[test]
    fn no_where_clause_names_the_missing_chain() {
        let err = translate_one("SELECT nonce FROM accounts")
            .unwrap_err()
            .to_string();
        assert!(err.contains("chain"), "{err}");
    }

    #[test]
    fn values_body_is_rejected_by_name() {
        let err = translate_one("VALUES (1, 2, 3)").unwrap_err().to_string();
        assert!(err.contains("VALUES"), "{err}");
    }

    #[test]
    fn subquery_from_is_rejected_by_name() {
        let err =
            translate_one("SELECT nonce FROM (SELECT 1) WHERE address = ian.eth AND chain = eth")
                .unwrap_err()
                .to_string();
        assert!(err.contains("SELECT 1"), "{err}");
    }

    // `Query`/`Select` clauses that parse successfully under `DuckDbDialect`
    // but weren't read anywhere in this module — each would otherwise
    // translate to a normal `GetExpression` with the clause silently
    // dropped. One test per construct, asserting the error names it (not
    // just that translation failed).

    #[test]
    fn top_is_rejected_by_name() {
        let err = translate_one(
            "SELECT TOP 5 nonce FROM accounts WHERE address = ian.eth AND chain = eth",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("TOP 5"), "{err}");
    }

    #[test]
    fn into_is_rejected_by_name() {
        let err = translate_one(
            "SELECT nonce INTO foo FROM accounts WHERE address = ian.eth AND chain = eth",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("INTO foo"), "{err}");
    }

    #[test]
    fn lateral_view_is_rejected_by_name() {
        let err = translate_one(
            "SELECT nonce FROM accounts LATERAL VIEW explode(x) t AS y WHERE address = ian.eth AND chain = eth",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("LATERAL VIEW"), "{err}");
    }

    #[test]
    fn cluster_by_is_rejected_by_name() {
        let err = translate_one(
            "SELECT nonce FROM accounts WHERE address = ian.eth AND chain = eth CLUSTER BY nonce",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("CLUSTER BY") && err.contains("nonce"), "{err}");
    }

    #[test]
    fn distribute_by_is_rejected_by_name() {
        let err = translate_one(
            "SELECT nonce FROM accounts WHERE address = ian.eth AND chain = eth DISTRIBUTE BY nonce",
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("DISTRIBUTE BY") && err.contains("nonce"),
            "{err}"
        );
    }

    #[test]
    fn sort_by_is_rejected_by_name() {
        let err = translate_one(
            "SELECT nonce FROM accounts WHERE address = ian.eth AND chain = eth SORT BY nonce",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("SORT BY") && err.contains("nonce"), "{err}");
    }

    #[test]
    fn qualify_is_rejected_by_name() {
        let err = translate_one(
            "SELECT nonce FROM accounts WHERE address = ian.eth AND chain = eth QUALIFY nonce > 1",
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("QUALIFY") && err.contains("nonce > 1"),
            "{err}"
        );
    }

    #[test]
    fn window_is_rejected_by_name() {
        let err = translate_one(
            "SELECT nonce FROM accounts WHERE address = ian.eth AND chain = eth WINDOW w AS (PARTITION BY nonce)",
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("WINDOW") && err.contains("PARTITION BY"),
            "{err}"
        );
    }

    #[test]
    fn cte_with_is_rejected_by_name() {
        let err = translate_one(
            "WITH cte AS (SELECT 1) SELECT nonce FROM accounts WHERE address = ian.eth AND chain = eth",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("WITH") && err.contains("cte"), "{err}");
    }

    #[test]
    fn fetch_is_rejected_by_name() {
        let err = translate_one(
            "SELECT nonce FROM accounts WHERE address = ian.eth AND chain = eth FETCH FIRST 5 ROWS ONLY",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("FETCH FIRST"), "{err}");
    }

    #[test]
    fn locking_clause_is_rejected_by_name() {
        let err = translate_one(
            "SELECT nonce FROM accounts WHERE address = ian.eth AND chain = eth FOR UPDATE",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("FOR UPDATE"), "{err}");
    }

    #[test]
    fn for_json_clause_is_rejected_by_name() {
        let err = translate_one(
            "SELECT nonce FROM accounts WHERE address = ian.eth AND chain = eth FOR JSON AUTO",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("FOR JSON"), "{err}");
    }

    // The remaining `Query`/`Select` fields below (`prewhere`, `limit_by`,
    // `settings`, `format_clause`, `value_table_mode`, `connect_by`) are
    // confirmed unreachable through this module's only entry point: each is
    // gated in `sqlparser`'s parser behind a dialect other than
    // `DuckDbDialect` (verified by reading `sqlparser` 0.52.0's parser
    // source), so `translate_one` can never produce a non-default value for
    // them. They're still checked in `validate_select_shape`/`query_to_get`
    // for defense-in-depth (see the module doc comment), so we exercise that
    // defensive code directly by building the AST by hand — the same
    // technique `where_clause`'s `rejects_empty_in_list` test uses for an
    // input shape the parser itself refuses to produce.

    fn base_query(sql: &str) -> sqlparser::ast::Query {
        let prelexed = crate::interpreter::frontend::sql::prelex::prelex(sql).unwrap();
        let stmts =
            sqlparser::parser::Parser::parse_sql(&sqlparser::dialect::DuckDbDialect {}, &prelexed)
                .unwrap();
        match stmts.into_iter().next().unwrap() {
            Statement::Query(q) => *q,
            other => panic!("not a query: {other}"),
        }
    }

    fn base_select(query: &mut sqlparser::ast::Query) -> &mut Select {
        match &mut *query.body {
            SetExpr::Select(select) => select,
            other => panic!("not a select: {other}"),
        }
    }

    #[test]
    fn prewhere_is_rejected_defensively() {
        let mut query =
            base_query("SELECT nonce FROM accounts WHERE address = ian.eth AND chain = eth");
        base_select(&mut query).prewhere = Some(Expr::Identifier(sqlparser::ast::Ident::new("x")));
        let err = query_to_get(&query, None).unwrap_err().to_string();
        assert!(err.contains("PREWHERE"), "{err}");
    }

    #[test]
    fn limit_by_is_rejected_defensively() {
        let mut query =
            base_query("SELECT nonce FROM accounts WHERE address = ian.eth AND chain = eth");
        query.limit_by = vec![Expr::Identifier(sqlparser::ast::Ident::new("nonce"))];
        let err = query_to_get(&query, None).unwrap_err().to_string();
        assert!(err.contains("BY") && err.contains("nonce"), "{err}");
    }

    #[test]
    fn settings_is_rejected_defensively() {
        let mut query =
            base_query("SELECT nonce FROM accounts WHERE address = ian.eth AND chain = eth");
        query.settings = Some(vec![sqlparser::ast::Setting {
            key: sqlparser::ast::Ident::new("max_threads"),
            value: sqlparser::ast::Value::Number("1".into(), false),
        }]);
        let err = query_to_get(&query, None).unwrap_err().to_string();
        assert!(err.contains("SETTINGS"), "{err}");
    }

    #[test]
    fn format_clause_is_rejected_defensively() {
        let mut query =
            base_query("SELECT nonce FROM accounts WHERE address = ian.eth AND chain = eth");
        query.format_clause = Some(sqlparser::ast::FormatClause::Identifier(
            sqlparser::ast::Ident::new("JSON"),
        ));
        let err = query_to_get(&query, None).unwrap_err().to_string();
        assert!(err.contains("FORMAT"), "{err}");
    }

    #[test]
    fn value_table_mode_is_rejected_defensively() {
        let mut query =
            base_query("SELECT nonce FROM accounts WHERE address = ian.eth AND chain = eth");
        base_select(&mut query).value_table_mode = Some(sqlparser::ast::ValueTableMode::AsStruct);
        let err = query_to_get(&query, None).unwrap_err().to_string();
        assert!(err.contains("STRUCT"), "{err}");
    }

    #[test]
    fn connect_by_is_rejected_defensively() {
        let mut query =
            base_query("SELECT nonce FROM accounts WHERE address = ian.eth AND chain = eth");
        base_select(&mut query).connect_by = Some(sqlparser::ast::ConnectBy {
            condition: Expr::Identifier(sqlparser::ast::Ident::new("x")),
            relationships: vec![Expr::Identifier(sqlparser::ast::Ident::new("y"))],
        });
        let err = query_to_get(&query, None).unwrap_err().to_string();
        assert!(err.contains("CONNECT BY"), "{err}");
    }
}
