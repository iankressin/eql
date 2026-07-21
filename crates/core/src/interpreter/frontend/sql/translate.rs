//! Translates a parsed SQL `Statement` into the existing `Expression` /
//! `GetExpression` / `Entity` structs the backend already executes.
//!
//! Covers all four entities (`accounts`, `blocks`, `transactions`/`tx`,
//! `logs`). This module also owns every statement-level rejection that
//! `where_clause` can't see.
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
    chain::Chain,
    dump::{Dump, DumpFormat},
    ens::NameOrAddress,
    entity::Entity,
    filters::{ComparisonFilter, EqualityFilter, FilterType},
    logs::{LogField, LogFilter, Logs},
    transaction::{Transaction, TransactionField, TransactionFilter},
    types::{Expression, GetExpression, SetRpcExpression},
};
use alloy::transports::http::reqwest::Url;
use sqlparser::ast::{
    CopySource, CopyTarget, Expr, Select, SelectItem, SetExpr, Statement, TableFactor,
};
use std::collections::HashMap;
use std::fmt::Display;
use std::str::FromStr;

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
        // Exhaustive over every `Statement::Copy` field — see `copy_to_expression`.
        Statement::Copy {
            source,
            to,
            target,
            options,
            legacy_options,
            values,
        } => copy_to_expression(source, *to, target, options, legacy_options, values),
        // Exhaustive over every `Statement::SetVariable` field — see
        // `set_variable_to_expression`.
        Statement::SetVariable {
            local,
            hivevar,
            variables,
            value,
        } => set_variable_to_expression(*local, *hivevar, variables, value),
        other => Err(EqlSqlError::NotSupported(format!("statement {other}"))),
    }
}

/// Translates `COPY (SELECT …) TO '<name>.<ext>'` into a `Get` expression
/// carrying a `Dump` — the SQL spelling of the legacy `>> file` dump syntax.
///
/// Every `Statement::Copy` field is checked by name rather than skipped via
/// `..` (see the module doc comment): `options`, `legacy_options` and
/// `values` aren't meaningful for the one shape EQL supports (copying a
/// `SELECT` out to a file), but a `COPY (SELECT …) TO 'x.json' (FORMAT csv)`
/// that silently dropped the `FORMAT csv` option would be exactly the
/// silent-discard bug this module's exhaustive-destructure convention
/// exists to prevent.
fn copy_to_expression(
    source: &CopySource,
    to: bool,
    target: &CopyTarget,
    options: &[sqlparser::ast::CopyOption],
    legacy_options: &[sqlparser::ast::CopyLegacyOption],
    values: &[Option<String>],
) -> Result<Expression, EqlSqlError> {
    // Only ever populated for `COPY ... FROM STDIN` (inline TSV rows
    // following the statement, per `sqlparser`'s `parse_copy`); unreachable
    // when `to` is true, but checked defensively rather than dropped.
    if !values.is_empty() {
        return Err(EqlSqlError::NotSupported(
            "COPY ... FROM STDIN inline values".into(),
        ));
    }
    if !options.is_empty() {
        return Err(EqlSqlError::NotSupported(format!(
            "COPY options ({})",
            joined(options)
        )));
    }
    if !legacy_options.is_empty() {
        return Err(EqlSqlError::NotSupported(format!(
            "COPY options ({})",
            joined(legacy_options)
        )));
    }
    if !to {
        return Err(EqlSqlError::NotSupported(
            "COPY ... FROM (import); EQL only supports COPY (SELECT …) TO …".into(),
        ));
    }
    let query = match source {
        CopySource::Query(query) => query,
        CopySource::Table {
            table_name,
            columns: _, // only meaningful for `COPY FROM`, which we reject above
        } => {
            return Err(EqlSqlError::NotSupported(format!(
                "COPY of table {table_name}; wrap a SELECT: COPY (SELECT …) TO '…'"
            )))
        }
    };
    let filename = match target {
        CopyTarget::File { filename } => filename,
        other => return Err(EqlSqlError::NotSupported(format!("COPY TO {other}"))),
    };
    let (name, ext) = filename.rsplit_once('.').ok_or_else(|| {
        EqlSqlError::Validation("export file needs a .json, .csv or .parquet extension".into())
    })?;
    let format = DumpFormat::try_from(ext).map_err(|e| EqlSqlError::Validation(e.to_string()))?;
    query_to_get(query, Some(Dump::new(name.to_string(), format)))
}

/// Translates `SET rpc_<chain> = '<url>'` into `Expression::Set`, a
/// session-scoped RPC override applied by the execution engine (not
/// resolved into rows the way `Get` is — see `SetRpcExpression`'s doc
/// comment for what "session-scoped" means here).
///
/// `local`/`hivevar` and multi-variable/multi-value forms are rejected by
/// name: EQL has no notion of a `LOCAL`-scoped or Hive-style variable, and a
/// `SET rpc_eth = 'a', 'b'` naming two values for one variable has no
/// sensible translation, so both are rejected rather than silently taking
/// the first value.
fn set_variable_to_expression(
    local: bool,
    hivevar: bool,
    variables: &sqlparser::ast::OneOrManyWithParens<sqlparser::ast::ObjectName>,
    value: &[Expr],
) -> Result<Expression, EqlSqlError> {
    if local {
        return Err(EqlSqlError::NotSupported("SET LOCAL".into()));
    }
    if hivevar {
        return Err(EqlSqlError::NotSupported("SET HIVEVAR:...".into()));
    }
    let variable = variables_single_name(variables)?;
    let chain_name = variable
        .strip_prefix("rpc_")
        .ok_or_else(|| EqlSqlError::NotSupported(format!("SET {variable}")))?;
    let chain = Chain::try_from(chain_name).map_err(|e| EqlSqlError::Validation(e.to_string()))?;
    if value.len() > 1 {
        return Err(EqlSqlError::NotSupported(format!(
            "SET {variable} with multiple values ({})",
            joined(value)
        )));
    }
    let value_expr = value
        .first()
        .ok_or_else(|| EqlSqlError::Validation(format!("SET {variable} needs a value")))?;
    let url_text = values::expr_as_string(value_expr)?;
    let url = Url::parse(&url_text)
        .map_err(|e| EqlSqlError::Validation(format!("invalid url '{url_text}': {e}")))?;
    Ok(Expression::Set(SetRpcExpression { chain, url }))
}

/// `variables` is `Many(...)` only for `SET (a, b) = (1, 2)`, syntax gated
/// behind `Dialect::supports_parenthesized_set_variables()`, which
/// `DuckDbDialect` does not implement — unreachable through `translate_one`
/// today, but checked defensively rather than dropped (see the module doc
/// comment).
fn variables_single_name(
    variables: &sqlparser::ast::OneOrManyWithParens<sqlparser::ast::ObjectName>,
) -> Result<String, EqlSqlError> {
    use sqlparser::ast::OneOrManyWithParens;
    match variables {
        OneOrManyWithParens::One(name) => Ok(name.to_string().to_ascii_lowercase()),
        OneOrManyWithParens::Many(names) => Err(EqlSqlError::NotSupported(format!(
            "SET ({})",
            joined(names)
        ))),
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

/// Renders a `CondOp` the way the user wrote it, for error messages that
/// must name the actual operator rather than a fixed placeholder — mirrors
/// `where_clause`'s private `cond_op_text`, duplicated here rather than
/// exported since it's a one-line lookup and `where_clause` is otherwise a
/// stable, already-reviewed dependency of this module.
fn op_text(op: CondOp) -> &'static str {
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

/// Builds an `EqualityFilter` for columns that only ever support `=`/`!=`
/// (identity-like columns: addresses, type, status, y_parity, data). Any
/// other operator is rejected by name — both the column (`what`) and the
/// actual operator the user wrote.
fn eq_only<T>(op: CondOp, value: T, what: &str) -> Result<EqualityFilter<T>, EqlSqlError> {
    match op {
        CondOp::Eq => Ok(EqualityFilter::Eq(value)),
        CondOp::Neq => Ok(EqualityFilter::Neq(value)),
        other => Err(EqlSqlError::NotSupported(format!(
            "{what} {} (only = and != are supported)",
            op_text(other)
        ))),
    }
}

/// Builds a `FilterType` for numeric columns that support the full
/// equality+comparison set but not `IN`/`BETWEEN` (no `TransactionFilter`
/// variant carries a list or a range for these columns).
fn cmp_filter<T>(op: CondOp, value: T, what: &str) -> Result<FilterType<T>, EqlSqlError> {
    Ok(match op {
        CondOp::Eq => FilterType::Equality(EqualityFilter::Eq(value)),
        CondOp::Neq => FilterType::Equality(EqualityFilter::Neq(value)),
        CondOp::Gt => FilterType::Comparison(ComparisonFilter::Gt(value)),
        CondOp::Gte => FilterType::Comparison(ComparisonFilter::Gte(value)),
        CondOp::Lt => FilterType::Comparison(ComparisonFilter::Lt(value)),
        CondOp::Lte => FilterType::Comparison(ComparisonFilter::Lte(value)),
        other @ (CondOp::In | CondOp::Between) => {
            return Err(EqlSqlError::NotSupported(format!(
                "{what} {} (only =, !=, >, >=, <, <= are supported)",
                op_text(other)
            )))
        }
    })
}

fn tx_address(cond: &Condition) -> Result<alloy::primitives::Address, EqlSqlError> {
    match values::parse_name_or_address(&cond.values[0])? {
        NameOrAddress::Address(address) => Ok(address),
        NameOrAddress::Name(_) => Err(EqlSqlError::NotSupported(
            "ENS names outside accounts.address".into(),
        )),
    }
}

/// `data` has no dedicated parser in `values` (it's the only transaction
/// filter column typed as raw bytes rather than a fixed-width value), so
/// it's parsed locally rather than growing `values`'s public surface for a
/// single caller.
fn tx_data(cond: &Condition) -> Result<alloy::primitives::Bytes, EqlSqlError> {
    let s = values::expr_as_string(&cond.values[0])?;
    alloy::primitives::Bytes::from_str(&s)
        .map_err(|e| EqlSqlError::Validation(format!("invalid data '{s}': {e}")))
}

/// Pushes a `TransactionFilter::BlockId`, rejecting a second one by name.
/// Two `BlockId` filters can't both be honored: the backend
/// (`Transaction::get_block_id_filter`) picks the *first* one it finds and
/// silently ignores any other, so letting a second one through here would
/// silently discard whatever block predicate the user wrote second (e.g.
/// `block_number = 1 AND block_number BETWEEN 2 AND 3`).
fn push_block_id_filter(
    filters: &mut Vec<TransactionFilter>,
    block_id: BlockId,
) -> Result<(), EqlSqlError> {
    if filters
        .iter()
        .any(|f| matches!(f, TransactionFilter::BlockId(_)))
    {
        return Err(EqlSqlError::NotSupported(
            "transactions.block_number given more than once".into(),
        ));
    }
    filters.push(TransactionFilter::BlockId(block_id));
    Ok(())
}

fn build_transaction(fields: &[String], conds: Vec<Condition>) -> Result<Entity, EqlSqlError> {
    let fields = if fields == ["*"] {
        TransactionField::all_variants().to_vec()
    } else {
        fields
            .iter()
            .map(|f| schema::resolve_transaction_field(f))
            .collect::<Result<Vec<_>, _>>()?
    };

    let mut ids: Vec<alloy::primitives::B256> = Vec::new();
    let mut filters: Vec<TransactionFilter> = Vec::new();

    for cond in &conds {
        match (cond.column.as_str(), cond.op) {
            ("hash", CondOp::Eq) | ("hash", CondOp::In) => {
                for value in &cond.values {
                    ids.push(values::parse_b256(value)?);
                }
            }
            ("block_number", CondOp::Eq) => push_block_id_filter(
                &mut filters,
                BlockId::Number(values::parse_block_number_or_tag(&cond.values[0])?),
            )?,
            ("block_number", CondOp::Between) => push_block_id_filter(
                &mut filters,
                BlockId::Range(BlockRange::new(
                    values::parse_block_number_or_tag(&cond.values[0])?,
                    Some(values::parse_block_number_or_tag(&cond.values[1])?),
                )),
            )?,
            ("from_address", _) => filters.push(TransactionFilter::From(eq_only(
                cond.op,
                tx_address(cond)?,
                "from_address",
            )?)),
            ("to_address", _) => filters.push(TransactionFilter::To(eq_only(
                cond.op,
                tx_address(cond)?,
                "to_address",
            )?)),
            ("value", _) => filters.push(TransactionFilter::Value(cmp_filter(
                cond.op,
                values::parse_u256(&cond.values[0])?,
                "value",
            )?)),
            ("gas_price", _) => filters.push(TransactionFilter::GasPrice(cmp_filter(
                cond.op,
                values::parse_u128(&cond.values[0])?,
                "gas_price",
            )?)),
            ("gas_limit", _) => filters.push(TransactionFilter::GasLimit(cmp_filter(
                cond.op,
                values::parse_u64(&cond.values[0])?,
                "gas_limit",
            )?)),
            ("effective_gas_price", _) => {
                filters.push(TransactionFilter::EffectiveGasPrice(cmp_filter(
                    cond.op,
                    values::parse_u128(&cond.values[0])?,
                    "effective_gas_price",
                )?))
            }
            ("max_fee_per_gas", _) => filters.push(TransactionFilter::MaxFeePerGas(cmp_filter(
                cond.op,
                values::parse_u128(&cond.values[0])?,
                "max_fee_per_gas",
            )?)),
            ("max_fee_per_blob_gas", _) => {
                filters.push(TransactionFilter::MaxFeePerBlobGas(cmp_filter(
                    cond.op,
                    values::parse_u128(&cond.values[0])?,
                    "max_fee_per_blob_gas",
                )?))
            }
            ("max_priority_fee_per_gas", _) => {
                filters.push(TransactionFilter::MaxPriorityFeePerGas(cmp_filter(
                    cond.op,
                    values::parse_u128(&cond.values[0])?,
                    "max_priority_fee_per_gas",
                )?))
            }
            ("type", _) => filters.push(TransactionFilter::Type(eq_only(
                cond.op,
                values::parse_u8(&cond.values[0])?,
                "type",
            )?)),
            ("status", _) => filters.push(TransactionFilter::Status(eq_only(
                cond.op,
                values::parse_bool(&cond.values[0])?,
                "status",
            )?)),
            ("y_parity", _) => filters.push(TransactionFilter::YParity(eq_only(
                cond.op,
                values::parse_bool(&cond.values[0])?,
                "y_parity",
            )?)),
            ("data", _) => filters.push(TransactionFilter::Data(eq_only(
                cond.op,
                tx_data(cond)?,
                "data",
            )?)),
            (col, op) => {
                return Err(EqlSqlError::NotSupported(format!(
                    "filter on transactions.{col} {}",
                    op_text(op)
                )))
            }
        }
    }

    let has_block = filters
        .iter()
        .any(|f| matches!(f, TransactionFilter::BlockId(_)));
    if ids.is_empty() && !has_block {
        return Err(EqlSqlError::Validation(
            "transactions queries need hash (=/IN) or block_number (=/BETWEEN)".into(),
        ));
    }
    Ok(Entity::Transaction(Transaction::new(
        if ids.is_empty() { None } else { Some(ids) },
        if filters.is_empty() {
            None
        } else {
            Some(filters)
        },
        fields,
    )))
}

fn log_eq<'a>(cond: &'a Condition, what: &str) -> Result<&'a Expr, EqlSqlError> {
    if cond.op != CondOp::Eq {
        return Err(EqlSqlError::NotSupported(format!(
            "logs.{what} {} (only = is supported)",
            op_text(cond.op)
        )));
    }
    Ok(&cond.values[0])
}

/// Rejects a second occurrence of the same log filter column by name. Every
/// `LogFilter` variant ends up as a single slot in a downstream builder — an
/// `alloy::rpc::types::Filter` for the RPC path (`Logs::build_filter`'s
/// `.address()`/`.topic1()`/... each simply overwrite the previous value)
/// and a `serde_json::Map` keyed by column for the Portal path
/// (`resolve_logs_via_portal` inserts by the same key) — so a second filter
/// on the same column doesn't compose with the first, it silently replaces
/// it (or, for `block_number`, is silently ignored: `find_block_range` keeps
/// only the first match). Rejecting outright means the user's second
/// condition is never quietly dropped.
fn reject_duplicate_log_filter(
    filters: &[LogFilter],
    col: &str,
    already_present: impl Fn(&LogFilter) -> bool,
) -> Result<(), EqlSqlError> {
    if filters.iter().any(already_present) {
        return Err(EqlSqlError::NotSupported(format!(
            "logs.{col} given more than once"
        )));
    }
    Ok(())
}

fn build_logs(fields: &[String], conds: Vec<Condition>) -> Result<Entity, EqlSqlError> {
    let fields = if fields == ["*"] {
        LogField::all_variants().to_vec()
    } else {
        fields
            .iter()
            .map(|f| schema::resolve_log_field(f))
            .collect::<Result<Vec<_>, _>>()?
    };

    let mut filters: Vec<LogFilter> = Vec::new();
    for cond in &conds {
        match cond.column.as_str() {
            "address" => {
                reject_duplicate_log_filter(&filters, "address", |f| {
                    matches!(f, LogFilter::EmitterAddress(_))
                })?;
                filters.push(LogFilter::EmitterAddress(values::parse_address(log_eq(
                    cond, "address",
                )?)?));
            }
            "topic0" => {
                reject_duplicate_log_filter(&filters, "topic0", |f| {
                    matches!(f, LogFilter::Topic0(_))
                })?;
                filters.push(LogFilter::Topic0(values::parse_b256(log_eq(
                    cond, "topic0",
                )?)?));
            }
            "topic1" => {
                reject_duplicate_log_filter(&filters, "topic1", |f| {
                    matches!(f, LogFilter::Topic1(_))
                })?;
                filters.push(LogFilter::Topic1(values::parse_b256(log_eq(
                    cond, "topic1",
                )?)?));
            }
            "topic2" => {
                reject_duplicate_log_filter(&filters, "topic2", |f| {
                    matches!(f, LogFilter::Topic2(_))
                })?;
                filters.push(LogFilter::Topic2(values::parse_b256(log_eq(
                    cond, "topic2",
                )?)?));
            }
            "topic3" => {
                reject_duplicate_log_filter(&filters, "topic3", |f| {
                    matches!(f, LogFilter::Topic3(_))
                })?;
                filters.push(LogFilter::Topic3(values::parse_b256(log_eq(
                    cond, "topic3",
                )?)?));
            }
            "block_hash" => {
                reject_duplicate_log_filter(&filters, "block_hash", |f| {
                    matches!(f, LogFilter::BlockHash(_))
                })?;
                filters.push(LogFilter::BlockHash(values::parse_b256(log_eq(
                    cond,
                    "block_hash",
                )?)?));
            }
            "event_signature" => {
                reject_duplicate_log_filter(&filters, "event_signature", |f| {
                    matches!(f, LogFilter::EventSignature(_))
                })?;
                filters.push(LogFilter::EventSignature(values::expr_as_string(log_eq(
                    cond,
                    "event_signature",
                )?)?));
            }
            "block_number" => match cond.op {
                CondOp::Eq => {
                    reject_duplicate_log_filter(&filters, "block_number", |f| {
                        matches!(f, LogFilter::BlockRange(_))
                    })?;
                    filters.push(LogFilter::BlockRange(BlockRange::new(
                        values::parse_block_number_or_tag(&cond.values[0])?,
                        None,
                    )));
                }
                CondOp::Between => {
                    reject_duplicate_log_filter(&filters, "block_number", |f| {
                        matches!(f, LogFilter::BlockRange(_))
                    })?;
                    filters.push(LogFilter::BlockRange(BlockRange::new(
                        values::parse_block_number_or_tag(&cond.values[0])?,
                        Some(values::parse_block_number_or_tag(&cond.values[1])?),
                    )));
                }
                other => {
                    return Err(EqlSqlError::NotSupported(format!(
                        "logs.block_number {} (only = and BETWEEN are supported)",
                        op_text(other)
                    )))
                }
            },
            col => {
                return Err(EqlSqlError::NotSupported(format!(
                    "filter on logs.{col} {}",
                    op_text(cond.op)
                )))
            }
        }
    }

    let has_block = filters
        .iter()
        .any(|f| matches!(f, LogFilter::BlockRange(_) | LogFilter::BlockHash(_)));
    if !has_block {
        return Err(EqlSqlError::Validation(
            "logs queries need block_number (=/BETWEEN) or block_hash".into(),
        ));
    }
    Ok(Entity::Logs(Logs::new(filters, fields)))
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
        let Expression::Get(get) = expr else {
            panic!("not a Get")
        };
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
        let Expression::Get(get) = expr else {
            panic!("not a Get")
        };
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
        let Expression::Get(get) = expr else {
            panic!("not a Get")
        };
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
        let Expression::Get(get) = expr else {
            panic!("not a Get")
        };
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
        let Expression::Get(get) = expr else {
            panic!("not a Get")
        };
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
        let Expression::Get(get) = expr else {
            panic!("not a Get")
        };
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
        let Expression::Get(get) = expr else {
            panic!("not a Get")
        };
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
        let Expression::Get(get) = expr else {
            panic!("not a Get")
        };
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

    // Task 7: transactions and logs translation.

    #[test]
    fn tx_by_hash() {
        let expr = translate_one(
            "SELECT * FROM tx WHERE hash = 0x6f93d4add2ef6cdfbb9f25b9895830d719dd8edf6637b639d5c33e808ded4247 AND chain = eth",
        )
        .unwrap();
        let Expression::Get(get) = expr else {
            panic!("not a Get")
        };
        let crate::common::entity::Entity::Transaction(tx) = get.entity else {
            panic!()
        };
        assert_eq!(tx.ids().unwrap().len(), 1);
    }

    #[test]
    fn tx_by_block_with_value_filter() {
        use crate::common::{
            filters::{ComparisonFilter, FilterType},
            transaction::TransactionFilter,
        };
        use alloy::primitives::U256;
        let expr = translate_one(
            "SELECT from_address, value FROM transactions WHERE block_number = latest AND value > 1 ether AND chain = eth",
        )
        .unwrap();
        let Expression::Get(get) = expr else {
            panic!("not a Get")
        };
        let crate::common::entity::Entity::Transaction(tx) = get.entity else {
            panic!()
        };
        let filters = tx.filters().unwrap();
        assert!(filters
            .iter()
            .any(|f| matches!(f, TransactionFilter::BlockId(_))));
        assert!(filters.iter().any(|f| matches!(
            f,
            TransactionFilter::Value(FilterType::Comparison(ComparisonFilter::Gt(v)))
                if *v == U256::from(10).pow(U256::from(18))
        )));
    }

    #[test]
    fn tx_requires_hash_or_block() {
        let err = translate_one("SELECT value FROM tx WHERE value > 0 AND chain = eth")
            .unwrap_err()
            .to_string();
        assert!(err.contains("block_number") || err.contains("hash"));
    }

    #[test]
    fn tx_ens_in_address_filter_not_supported_yet() {
        let err = translate_one(
            "SELECT value FROM tx WHERE block_number = latest AND from_address = vitalik.eth AND chain = eth",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("ENS"));
    }

    #[test]
    fn logs_full_filter_set() {
        use crate::common::logs::LogFilter;
        let expr = translate_one(
            "SELECT * FROM logs WHERE address = 0xdAC17F958D2ee523a2206206994597C13D831ec7 \
             AND topic0 = 0xcb8241adb0c3fdb35b70c24ce35c5eb0c17af7431c99f827d44a445ca624176a \
             AND block_number BETWEEN 4638657 AND 4638758 AND chain = eth",
        )
        .unwrap();
        let Expression::Get(get) = expr else {
            panic!("not a Get")
        };
        let crate::common::entity::Entity::Logs(logs) = get.entity else {
            panic!()
        };
        assert!(logs
            .filter()
            .iter()
            .any(|f| matches!(f, LogFilter::EmitterAddress(_))));
        assert!(logs
            .filter()
            .iter()
            .any(|f| matches!(f, LogFilter::Topic0(_))));
        assert!(logs
            .filter()
            .iter()
            .any(|f| matches!(f, LogFilter::BlockRange(_))));
    }

    #[test]
    fn logs_event_signature_and_required_block() {
        let expr = translate_one(
            "SELECT * FROM logs WHERE event_signature = 'Confirmation(address,uint256)' \
             AND block_number = 4638757 AND chain = eth",
        )
        .unwrap();
        let Expression::Get(get) = expr else {
            panic!("not a Get")
        };
        let crate::common::entity::Entity::Logs(logs) = get.entity else {
            panic!()
        };
        assert!(logs.filter().iter().any(|f| matches!(
            f,
            crate::common::logs::LogFilter::EventSignature(s) if s == "Confirmation(address,uint256)"
        )));

        let err = translate_one(
            "SELECT * FROM logs WHERE address = 0xdAC17F958D2ee523a2206206994597C13D831ec7 AND chain = eth",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("block"));
    }

    #[test]
    fn logs_reject_non_eq_operators() {
        let err = translate_one(
            "SELECT * FROM logs WHERE topic0 > 0xcb8241adb0c3fdb35b70c24ce35c5eb0c17af7431c99f827d44a445ca624176a AND block_number = 1 AND chain = eth",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("="));
    }

    // Filter reconciliation: `TransactionFilter::Data` exists and was
    // reachable through the legacy pest grammar's `data_filter` (an
    // `EqualityFilter<Bytes>` on the raw tx payload), but the brief's
    // reference implementation didn't wire it up. Added here to keep parity
    // with what a user could previously express.

    #[test]
    fn tx_data_filter_translates() {
        use crate::common::transaction::TransactionFilter;
        let expr = translate_one(
            "SELECT * FROM tx WHERE block_number = latest AND data = 0x1234 AND chain = eth",
        )
        .unwrap();
        let Expression::Get(get) = expr else {
            panic!("not a Get")
        };
        let crate::common::entity::Entity::Transaction(tx) = get.entity else {
            panic!()
        };
        assert!(tx
            .filters()
            .unwrap()
            .iter()
            .any(|f| matches!(f, TransactionFilter::Data(_))));
    }

    // Shapes the brief's tests don't cover. Each is either rejected clearly
    // (naming the real column and operator) or translated sensibly — never
    // mis-translated silently and never a panic.

    #[test]
    fn tx_hash_and_block_number_together_translates() {
        use crate::common::transaction::TransactionFilter;
        let expr = translate_one(
            "SELECT * FROM tx WHERE hash = 0x6f93d4add2ef6cdfbb9f25b9895830d719dd8edf6637b639d5c33e808ded4247 \
             AND block_number = latest AND chain = eth",
        )
        .unwrap();
        let Expression::Get(get) = expr else {
            panic!("not a Get")
        };
        let crate::common::entity::Entity::Transaction(tx) = get.entity else {
            panic!()
        };
        assert_eq!(tx.ids().unwrap().len(), 1);
        assert!(tx
            .filters()
            .unwrap()
            .iter()
            .any(|f| matches!(f, TransactionFilter::BlockId(_))));
    }

    #[test]
    fn tx_duplicate_block_number_is_rejected_clearly() {
        let err = translate_one(
            "SELECT * FROM tx WHERE block_number = 1 AND block_number BETWEEN 2 AND 3 AND chain = eth",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("block_number"), "{err}");
    }

    #[test]
    fn tx_value_range_via_two_conditions_translates() {
        // The same column given twice is fine for a comparison filter: both
        // conditions are ANDed by `Transaction::filter`, unlike `BlockId`
        // (see `push_block_id_filter`), so this must NOT be rejected.
        use crate::common::{filters::ComparisonFilter, transaction::TransactionFilter};
        let expr = translate_one(
            "SELECT * FROM tx WHERE block_number = latest AND value > 1 AND value < 100 AND chain = eth",
        )
        .unwrap();
        let Expression::Get(get) = expr else {
            panic!("not a Get")
        };
        let crate::common::entity::Entity::Transaction(tx) = get.entity else {
            panic!()
        };
        let filters = tx.filters().unwrap();
        assert!(filters.iter().any(|f| matches!(
            f,
            TransactionFilter::Value(crate::common::filters::FilterType::Comparison(
                ComparisonFilter::Gt(_)
            ))
        )));
        assert!(filters.iter().any(|f| matches!(
            f,
            TransactionFilter::Value(crate::common::filters::FilterType::Comparison(
                ComparisonFilter::Lt(_)
            ))
        )));
    }

    #[test]
    fn tx_to_address_in_is_rejected_clearly() {
        let err = translate_one(
            "SELECT * FROM tx WHERE block_number = latest \
             AND to_address IN (0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045, 0x0000000000000000000000000000000000000001) \
             AND chain = eth",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("to_address") && err.contains("IN"), "{err}");
    }

    #[test]
    fn tx_value_between_is_rejected_clearly() {
        let err = translate_one(
            "SELECT * FROM tx WHERE block_number = latest AND value BETWEEN 1 AND 2 AND chain = eth",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("value") && err.contains("BETWEEN"), "{err}");
    }

    #[test]
    fn tx_gas_limit_in_is_rejected_clearly() {
        let err = translate_one(
            "SELECT * FROM tx WHERE block_number = latest AND gas_limit IN (1, 2) AND chain = eth",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("gas_limit") && err.contains("IN"), "{err}");
    }

    #[test]
    fn tx_status_non_boolean_is_rejected_clearly() {
        let err = translate_one(
            "SELECT * FROM tx WHERE block_number = latest AND status = 1 AND chain = eth",
        )
        .unwrap_err()
        .to_string();
        // `1` parses as a number, not a `Value::Boolean`, so `values::parse_bool`
        // rejects it by shape rather than translating it as truthy.
        assert!(err.contains("true/false"), "{err}");
    }

    #[test]
    fn tx_unknown_filter_column_names_it() {
        let err = translate_one(
            "SELECT * FROM tx WHERE block_number = latest AND chain_id = 1 AND chain = eth",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("chain_id"), "{err}");
    }

    #[test]
    fn logs_duplicate_address_is_rejected_clearly() {
        let err = translate_one(
            "SELECT * FROM logs WHERE block_number = 1 \
             AND address = 0xdAC17F958D2ee523a2206206994597C13D831ec7 \
             AND address = 0x0000000000000000000000000000000000000001 AND chain = eth",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("address"), "{err}");
    }

    #[test]
    fn logs_duplicate_block_number_is_rejected_clearly() {
        let err = translate_one(
            "SELECT * FROM logs WHERE block_number = 1 AND block_number BETWEEN 2 AND 3 AND chain = eth",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("block_number"), "{err}");
    }

    #[test]
    fn logs_topic_wrong_length_hash_is_rejected_clearly() {
        let err = translate_one(
            "SELECT * FROM logs WHERE block_number = 1 \
             AND topic0 = 0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045 AND chain = eth",
        )
        .unwrap_err()
        .to_string();
        // A 20-byte address is the wrong length for a 32-byte topic hash;
        // `values::parse_b256` rejects it rather than truncating/padding it.
        assert!(err.contains("hash"), "{err}");
    }

    #[test]
    fn logs_unknown_filter_column_names_it() {
        let err = translate_one(
            "SELECT * FROM logs WHERE block_number = 1 AND removed = true AND chain = eth",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("removed"), "{err}");
    }

    // Task 8: `COPY ... TO` exports and `SET rpc_<chain>` session overrides.

    #[test]
    fn copy_to_becomes_dump() {
        use crate::common::dump::{Dump, DumpFormat};
        let expr = translate_one(
            "COPY (SELECT * FROM blocks WHERE number = 1 AND chain = eth) TO 'out/blocks.parquet'",
        )
        .unwrap();
        let Expression::Get(get) = expr else {
            panic!("not a Get")
        };
        assert_eq!(
            get.dump,
            Some(Dump::new("out/blocks".into(), DumpFormat::Parquet))
        );
    }

    #[test]
    fn copy_rejects_unknown_extension() {
        let err = translate_one(
            "COPY (SELECT * FROM blocks WHERE number = 1 AND chain = eth) TO 'out.xlsx'",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("json") || err.contains("format"));
    }

    #[test]
    fn set_rpc_translates() {
        let expr = translate_one("SET rpc_eth = 'https://my-node:8545'").unwrap();
        let Expression::Set(set) = expr else {
            panic!("not a Set")
        };
        assert_eq!(set.chain, crate::common::chain::Chain::Ethereum);
        assert_eq!(set.url.as_str(), "https://my-node:8545/");
    }

    #[test]
    fn set_unknown_variable_errors() {
        assert!(translate_one("SET foo = 'bar'").is_err());
        assert!(translate_one("SET rpc_nochain = 'https://x'").is_err());
    }

    // Shapes the brief's tests above don't cover. Each is either rejected
    // clearly (naming the real construct) or translated sensibly — never
    // mis-translated silently and never a panic.

    #[test]
    fn copy_rejects_path_with_no_extension() {
        let err = translate_one(
            "COPY (SELECT * FROM blocks WHERE number = 1 AND chain = eth) TO 'out/blocks'",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("extension"), "{err}");
    }

    #[test]
    fn copy_rejects_directory_like_path_without_a_real_extension() {
        // The last `.` in the path lands inside a directory segment, not on
        // a file extension; `DumpFormat::try_from` rejects the bogus
        // "extension" it gets handed rather than misinterpreting the path.
        let err = translate_one(
            "COPY (SELECT * FROM blocks WHERE number = 1 AND chain = eth) TO 'out.dir/blocks'",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("dir/blocks"), "{err}");
    }

    #[test]
    fn copy_rejects_extension_in_the_wrong_case() {
        // Extension matching is case-sensitive, same as the legacy pest
        // `Dump`/`DumpFormat` grammar this reuses — not a new restriction.
        let err = translate_one(
            "COPY (SELECT * FROM blocks WHERE number = 1 AND chain = eth) TO 'out.PARQUET'",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("PARQUET"), "{err}");
    }

    #[test]
    fn copy_to_stdout_is_rejected_by_name() {
        let err =
            translate_one("COPY (SELECT * FROM blocks WHERE number = 1 AND chain = eth) TO STDOUT")
                .unwrap_err()
                .to_string();
        assert!(err.contains("STDOUT"), "{err}");
    }

    #[test]
    fn copy_from_is_rejected_by_name() {
        let err = translate_one("COPY blocks FROM 'in.csv'")
            .unwrap_err()
            .to_string();
        assert!(err.contains("FROM"), "{err}");
    }

    #[test]
    fn copy_of_bare_table_names_the_table() {
        let err = translate_one("COPY blocks TO 'out.json'")
            .unwrap_err()
            .to_string();
        assert!(err.contains("blocks") && err.contains("SELECT"), "{err}");
    }

    #[test]
    fn copy_with_options_is_rejected_by_name() {
        let err = translate_one(
            "COPY (SELECT * FROM blocks WHERE number = 1 AND chain = eth) TO 'out.csv' (FORMAT csv)",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("FORMAT"), "{err}");
    }

    #[test]
    fn set_local_is_rejected_by_name() {
        let err = translate_one("SET LOCAL rpc_eth = 'https://my-node:8545'")
            .unwrap_err()
            .to_string();
        assert!(err.contains("LOCAL"), "{err}");
    }

    #[test]
    fn set_multiple_values_is_rejected_by_name() {
        let err = translate_one("SET rpc_eth = 'https://a', 'https://b'")
            .unwrap_err()
            .to_string();
        assert!(err.contains("multiple values"), "{err}");
    }

    #[test]
    fn set_non_string_value_is_rejected_clearly() {
        let err = translate_one("SET rpc_eth = 123").unwrap_err().to_string();
        assert!(err.contains("123"), "{err}");
    }

    #[test]
    fn set_invalid_url_is_rejected_clearly() {
        let err = translate_one("SET rpc_eth = 'not-a-url'")
            .unwrap_err()
            .to_string();
        assert!(err.contains("not-a-url"), "{err}");
    }

    #[test]
    fn set_accepts_any_url_scheme_by_design() {
        // `Url::parse` doesn't restrict schemes, and neither does the
        // legacy pest `rpc_url` rule (`types.rs`'s `Rule::rpc_url` arm) —
        // an unsupported scheme surfaces later as a normal provider error
        // when the override is actually used, not as a translation-time
        // rejection.
        let expr = translate_one("SET rpc_eth = 'ftp://my-node:21'").unwrap();
        let Expression::Set(set) = expr else {
            panic!("not a Set")
        };
        assert_eq!(set.url.scheme(), "ftp");
    }

    #[test]
    fn set_many_variables_is_rejected_defensively() {
        // `variables` is only ever `Many(...)` for `SET (a, b) = (1, 2)`,
        // syntax gated behind `Dialect::supports_parenthesized_set_variables`,
        // which `DuckDbDialect` does not implement — unreachable through
        // `translate_one` today (see the module doc comment for why we
        // still check it), so built by hand here the same way
        // `prewhere_is_rejected_defensively` exercises other
        // parser-unreachable shapes.
        use sqlparser::ast::{Ident, ObjectName, OneOrManyWithParens, Value};
        let stmt = Statement::SetVariable {
            local: false,
            hivevar: false,
            variables: OneOrManyWithParens::Many(vec![
                ObjectName(vec![Ident::new("rpc_eth")]),
                ObjectName(vec![Ident::new("rpc_op")]),
            ]),
            value: vec![Expr::Value(Value::SingleQuotedString("x".into()))],
        };
        let err = statement_to_expression(&stmt).unwrap_err().to_string();
        assert!(err.contains("rpc_eth") && err.contains("rpc_op"), "{err}");
    }
}
