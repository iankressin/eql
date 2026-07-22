//! Translates a legacy pest `GET ... FROM ... ON ...` query into its EQL 2
//! SQL equivalent, for use as the `suggestion` in `EqlSqlError::LegacySyntax`.
//!
//! `legacy_error` is a pass-through no-op for anything that isn't a legacy
//! `GET` query (so ordinary SQL keeps flowing through the normal SQL
//! frontend untouched). For a `GET` query it re-parses with the surviving
//! pest `Parser`, then `render`s the resulting `GetExpression` back out as
//! SQL text users can paste into the new frontend. This is the only
//! remaining consumer of the pest parser (see Task 11, which wires this in
//! as `parse_program`'s first check).
//!
//! `render`'s job is to reproduce the *exact* semantics the pest parser
//! already validated, spelled as SQL that `sql::translate` accepts — see
//! that module's doc comment for the rules being targeted here. Every
//! branch below was checked against `sql::translate`'s actual behavior
//! (not assumed), and the round-trip tests at the bottom feed each
//! rendered suggestion back through `prelex` + `sqlparser` +
//! `translate::statement_to_expression` to prove it.
use super::EqlSqlError;
use crate::common::{
    account::{Account, AccountField},
    block::{Block, BlockField, BlockFilter, BlockId, BlockRange},
    chain::{Chain, ChainOrRpc},
    entity::Entity,
    filters::{ComparisonFilter, EqualityFilter, FilterType},
    logs::{LogField, LogFilter, Logs},
    transaction::{Transaction, TransactionField, TransactionFilter},
    types::{Expression, GetExpression},
};
use crate::interpreter::frontend::parser::Parser;
use alloy::eips::BlockNumberOrTag;
use std::fmt::Display;

/// Returns `None` for anything that isn't a legacy `GET` query (so normal
/// SQL parsing proceeds untouched), or `Some(EqlSqlError::LegacySyntax)`
/// carrying the EQL 2 equivalent as its `suggestion`. When the source
/// starts with `GET` but the pest grammar itself can't parse it either,
/// the suggestion falls back to pointing at the docs.
pub fn legacy_error(source: &str) -> Option<EqlSqlError> {
    if !source.trim_start().to_ascii_uppercase().starts_with("GET ") {
        return None;
    }
    let suggestion = match Parser::new(source).parse_expressions() {
        Ok(expressions) => expressions
            .iter()
            .map(|e| match e {
                Expression::Get(get) => render(get),
                // `Parser::parse_expressions` only ever builds
                // `Expression::Get` (`Rule::get` is the only top-level
                // alternative `program` accepts, and its handler always
                // constructs a `Get`) — `Expression::Set` only comes from
                // `sql::translate`'s `SET rpc_<chain> = ...` path. Matched
                // exhaustively rather than assumed away: if the grammar
                // ever grows a `SET`-shaped production, this arm still
                // names what happened instead of silently vanishing it.
                Expression::Set(set) => {
                    format!("-- unexpected SET expression from the legacy parser: {set:?}")
                }
            })
            .collect::<Vec<_>>()
            .join(";\n"),
        Err(_) => "See docs/query.md for the SQL syntax.".to_string(),
    };
    Some(EqlSqlError::LegacySyntax { suggestion })
}

/// Renders a `BlockNumberOrTag` the way `values::parse_block_number_or_tag`
/// reads it back: a bare decimal for `Number`, or the lower-cased tag name
/// (`latest`, `earliest`, `pending`, `finalized`, `safe`) for everything
/// else. `BlockNumberOrTag`'s `#[derive(Debug)]` prints exactly the unit
/// variant's name with no wrapping (verified against `alloy-eips` 0.6.3's
/// `eip1898.rs`, and covered by `tag_matches_every_non_numeric_variant`
/// below), so lower-casing it reproduces those five spellings exactly.
fn tag(t: &BlockNumberOrTag) -> String {
    match t {
        BlockNumberOrTag::Number(n) => n.to_string(),
        other => format!("{other:?}").to_ascii_lowercase(),
    }
}

/// Renders a `BlockRange` as `{column} = <start>` (no end) or `{column}
/// BETWEEN <start> AND <end>` — shared by every place a legacy range shows
/// up: `BlockId::Range` (block/tx ids), `BlockFilter::Range` (the block
/// entity's `WHERE block = ...` filter), and `LogFilter::BlockRange`.
fn range_condition(column: &str, range: &BlockRange) -> String {
    let (start, end) = range.range();
    match end {
        Some(end) => format!("{column} BETWEEN {} AND {}", tag(&start), tag(&end)),
        None => format!("{column} = {}", tag(&start)),
    }
}

fn block_id_condition(column: &str, id: &BlockId) -> String {
    match id {
        BlockId::Number(n) => format!("{column} = {}", tag(n)),
        BlockId::Range(range) => range_condition(column, range),
    }
}

fn joined<T: Display>(items: &[T]) -> String {
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Renders a field list, collapsing to `*` only when `fields` is *exactly*
/// `all` — same elements, same order. A length-only check (`fields.len() ==
/// all.len()`) would also fire for a hand-picked list that happens to name
/// every field with one repeated and one missing, or every field spelled
/// out in a different order — both real shapes a user can type — and
/// silently substitute `*`, which selects a different set (or a
/// differently-ordered result) than what they asked for.
fn field_list<T>(fields: &[T], all: &[T]) -> String
where
    T: Display + PartialEq,
{
    if fields == all {
        "*".to_string()
    } else {
        joined(fields)
    }
}

fn ids_condition<T: Display>(column: &str, ids: &[T]) -> String {
    if ids.len() == 1 {
        format!("{column} = {}", ids[0])
    } else {
        format!("{column} IN ({})", joined(ids))
    }
}

fn chain_text(c: &ChainOrRpc) -> String {
    match c {
        ChainOrRpc::Chain(chain) => chain.to_string(),
        ChainOrRpc::Rpc(url) => format!("'{url}'"),
    }
}

/// True when `chains` is exactly `Chain::all_variants()`, in order — the
/// shape `Chain::from_selector("*")` produces for the legacy `ON *`
/// wildcard. Order- and count-sensitive for the same reason `field_list`
/// is: a query that happened to name every chain individually in some
/// other order is a different (if equivalent-looking) request, not the
/// wildcard, and collapsing it would be presumptuous.
fn is_full_chain_wildcard(chains: &[ChainOrRpc]) -> bool {
    let all = Chain::all_variants();
    chains.len() == all.len()
        && chains
            .iter()
            .zip(all)
            .all(|(actual, expected)| matches!(actual, ChainOrRpc::Chain(c) if c == expected))
}

fn chains_condition(chains: &[ChainOrRpc]) -> String {
    if is_full_chain_wildcard(chains) {
        return "chain = '*'".to_string();
    }
    let rendered: Vec<String> = chains.iter().map(chain_text).collect();
    if rendered.len() == 1 {
        format!("chain = {}", rendered[0])
    } else {
        format!("chain IN ({})", rendered.join(", "))
    }
}

fn eq_condition<T: Display>(column: &str, filter: &EqualityFilter<T>) -> String {
    match filter {
        EqualityFilter::Eq(v) => format!("{column} = {v}"),
        EqualityFilter::Neq(v) => format!("{column} != {v}"),
    }
}

fn cmp_condition<T: Display>(column: &str, filter: &FilterType<T>) -> String {
    match filter {
        FilterType::Equality(f) => eq_condition(column, f),
        FilterType::Comparison(c) => match c {
            ComparisonFilter::Gt(v) => format!("{column} > {v}"),
            ComparisonFilter::Gte(v) => format!("{column} >= {v}"),
            ComparisonFilter::Lt(v) => format!("{column} < {v}"),
            ComparisonFilter::Lte(v) => format!("{column} <= {v}"),
        },
    }
}

/// What rendering one entity produces: either the `(table, fields,
/// conditions)` that `render` assembles into `SELECT ... FROM ... WHERE
/// ...`, or — when the new frontend requires a predicate the legacy
/// grammar never did (`render_logs`/`render_transaction`'s missing-
/// predicate checks) — a plain-English explanation instead of a query
/// that would parse as SQL but fail the moment it's pasted in. A query the
/// user can't paste back in is bad; a query that *looks* runnable but
/// silently isn't is worse, because it's indistinguishable from a correct
/// suggestion until it's already been pasted and run.
enum Rendered {
    Query {
        table: &'static str,
        fields: String,
        conditions: Vec<String>,
    },
    NoEquivalent(String),
}

fn render_account(account: &Account) -> Rendered {
    let field_list_str = field_list(&account.fields(), AccountField::all_variants());
    let mut conditions = Vec::new();
    // `Account.id` is `Some(non-empty)` for every legacy query that
    // actually parses: `account_get`'s only reachable production is
    // `account_id_list` (`account_filter_list`, the `WHERE address = ...`
    // alternative, fails to parse today — a pre-existing pest grammar gap
    // unrelated to this task; see the task report). Checked with `if let`
    // rather than assumed, so a future grammar fix can't silently start
    // producing an address-less suggestion here.
    if let Some(ids) = account.ids() {
        if !ids.is_empty() {
            conditions.push(ids_condition("address", ids));
        }
    }
    Rendered::Query {
        table: "accounts",
        fields: field_list_str,
        conditions,
    }
}

fn render_block(block: &Block) -> Rendered {
    let field_list_str = field_list(block.fields(), BlockField::all_variants());
    let mut conditions = Vec::new();
    if let Some(ids) = block.ids() {
        for id in ids {
            conditions.push(block_id_condition("number", id));
        }
    }
    // The legacy grammar's other production for a block entity —
    // `block_filter_list` (`WHERE block = ...`) — populates `filter`
    // instead of `ids` (leaving `ids` as `Some(vec![])`). Matched
    // exhaustively (one variant, `BlockFilter::Range`) so this can't
    // silently vanish from the suggestion the way it used to: `ids()` and
    // `filters()` are independent `Option`s on `Block`, and only reading
    // the first one meant a `WHERE block = ...` legacy query rendered a
    // suggestion with the block predicate simply missing.
    if let Some(filters) = block.filters() {
        for filter in filters {
            match filter {
                BlockFilter::Range(range) => conditions.push(range_condition("number", range)),
            }
        }
    }
    Rendered::Query {
        table: "blocks",
        fields: field_list_str,
        conditions,
    }
}

fn render_transaction(tx: &Transaction) -> Rendered {
    let field_list_str = field_list(tx.fields(), TransactionField::all_variants());
    let mut conditions = Vec::new();
    let has_hash = match tx.ids() {
        Some(ids) if !ids.is_empty() => {
            conditions.push(ids_condition("hash", ids));
            true
        }
        _ => false,
    };
    let mut has_block = false;
    if let Some(filters) = tx.filters() {
        // At most one `BlockId` filter is ever rendered: the runtime
        // engine's own accessor, `Transaction::get_block_id_filter`, picks
        // the *first* `BlockId` it finds and ignores any later one, so a
        // second `block = ...` filter in the legacy query was already a
        // no-op under the old engine, not a second predicate that took
        // effect. Rendering only the first is a faithful translation of
        // what the legacy query actually did; rendering both would add a
        // second `block_number = ...` AND-clause that
        // `sql::translate::push_block_id_filter` rejects outright as a
        // duplicate — trading a query that worked (if ambiguously) for
        // one that errors.
        for filter in filters {
            match filter {
                TransactionFilter::BlockId(id) => {
                    if !has_block {
                        conditions.push(block_id_condition("block_number", id));
                        has_block = true;
                    }
                }
                TransactionFilter::Type(f) => conditions.push(eq_condition("type", f)),
                TransactionFilter::From(f) => conditions.push(eq_condition("from_address", f)),
                TransactionFilter::To(f) => conditions.push(eq_condition("to_address", f)),
                TransactionFilter::Data(f) => conditions.push(eq_condition("data", f)),
                TransactionFilter::Value(f) => conditions.push(cmp_condition("value", f)),
                TransactionFilter::GasPrice(f) => conditions.push(cmp_condition("gas_price", f)),
                TransactionFilter::GasLimit(f) => conditions.push(cmp_condition("gas_limit", f)),
                TransactionFilter::EffectiveGasPrice(f) => {
                    conditions.push(cmp_condition("effective_gas_price", f))
                }
                TransactionFilter::MaxFeePerBlobGas(f) => {
                    conditions.push(cmp_condition("max_fee_per_blob_gas", f))
                }
                TransactionFilter::MaxFeePerGas(f) => {
                    conditions.push(cmp_condition("max_fee_per_gas", f))
                }
                TransactionFilter::MaxPriorityFeePerGas(f) => {
                    conditions.push(cmp_condition("max_priority_fee_per_gas", f))
                }
                TransactionFilter::Status(f) => conditions.push(eq_condition("status", f)),
                TransactionFilter::YParity(f) => conditions.push(eq_condition("y_parity", f)),
                // Unreachable through the legacy grammar today: none of
                // `tx_filter`'s alternatives in `productions.pest`
                // construct `Hash`, `ChainId`, `V`, `R` or `S` (there is no
                // `hash_filter`/`chain_id_filter`/`v_filter`/`r_filter`/
                // `s_filter` production). Matched anyway, exhaustively, so
                // a future grammar addition can't add a silently-dropped
                // filter here without this match failing to compile.
                // `chain_id`/`v`/`r`/`s` aren't accepted as WHERE filters
                // by `sql::translate::build_transaction` either (a Task 7
                // decision, out of scope here), so this rendering is
                // untested by the round-trip suite — it can't be exercised
                // end-to-end via either frontend.
                TransactionFilter::Hash(f) => conditions.push(eq_condition("hash", f)),
                TransactionFilter::ChainId(f) => conditions.push(eq_condition("chain_id", f)),
                TransactionFilter::V(f) => conditions.push(eq_condition("v", f)),
                TransactionFilter::R(f) => conditions.push(eq_condition("r", f)),
                TransactionFilter::S(f) => conditions.push(eq_condition("s", f)),
            }
        }
    }
    // Mirrors `sql::translate::build_transaction`'s own requirement: a
    // transaction needs a hash or a block predicate. The legacy
    // `tx_filter_list` grammar never enforced this — a query filtering
    // only on e.g. `gas_price` parses fine — so this is a real gap, not a
    // rendering choice. There is no predicate for `render` to invent (the
    // user never wrote one), so rather than emit SQL that parses but then
    // fails `build_transaction`'s check with no context, say so plainly.
    if !has_hash && !has_block {
        return Rendered::NoEquivalent(format!(
            "EQL 2 has no equivalent for this query: transactions require a hash or a \
             block predicate (e.g. \"hash = <hash>\" or \"block_number = <n>\"), which this \
             legacy query never specified. Add one before it can be translated. \
             (requested fields: {field_list_str})"
        ));
    }
    Rendered::Query {
        table: "transactions",
        fields: field_list_str,
        conditions,
    }
}

fn render_logs(logs: &Logs) -> Rendered {
    let field_list_str = field_list(logs.fields(), LogField::all_variants());
    let mut conditions = Vec::new();
    let mut has_block = false;
    for filter in logs.filter() {
        conditions.push(match filter {
            LogFilter::EmitterAddress(a) => format!("address = {a}"),
            LogFilter::Topic0(t) => format!("topic0 = {t}"),
            LogFilter::Topic1(t) => format!("topic1 = {t}"),
            LogFilter::Topic2(t) => format!("topic2 = {t}"),
            LogFilter::Topic3(t) => format!("topic3 = {t}"),
            LogFilter::BlockHash(h) => {
                has_block = true;
                format!("block_hash = {h}")
            }
            LogFilter::EventSignature(s) => format!("event_signature = '{s}'"),
            LogFilter::BlockRange(range) => {
                has_block = true;
                range_condition("block_number", range)
            }
        });
    }
    // Mirrors `sql::translate::build_logs`'s own requirement: logs need a
    // `block_number` or `block_hash` predicate. The legacy
    // `log_filter_list` grammar never enforced this — a query filtering
    // only on e.g. `address` parses fine — so this is a real gap, not a
    // rendering choice. There is no predicate for `render` to invent (the
    // user never wrote one), so rather than emit SQL that parses but then
    // fails `build_logs`'s check with no context, say so plainly.
    if !has_block {
        return Rendered::NoEquivalent(format!(
            "EQL 2 has no equivalent for this query: logs require a block predicate \
             (e.g. \"block_number = <n>\", \"block_number BETWEEN <a> AND <b>\", or \
             \"block_hash = <hash>\"), which this legacy query never specified. Add one \
             before it can be translated. (requested fields: {field_list_str})"
        ));
    }
    Rendered::Query {
        table: "logs",
        fields: field_list_str,
        conditions,
    }
}

fn render(get: &GetExpression) -> String {
    let rendered = match &get.entity {
        Entity::Account(account) => render_account(account),
        Entity::Block(block) => render_block(block),
        Entity::Transaction(tx) => render_transaction(tx),
        Entity::Logs(logs) => render_logs(logs),
    };
    let (table, field_list_str, mut conditions) = match rendered {
        Rendered::Query {
            table,
            fields,
            conditions,
        } => (table, fields, conditions),
        Rendered::NoEquivalent(message) => return message,
    };
    conditions.push(chains_condition(&get.chains));
    let select = format!(
        "SELECT {field_list_str} FROM {table}\nWHERE {}",
        conditions.join("\n  AND ")
    );
    match &get.dump {
        Some(dump) => format!("COPY (\n{select}\n) TO '{}'", dump.path()),
        None => select,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds a rendered suggestion back through the real SQL frontend
    /// pipeline (`prelex` -> `sqlparser` -> `translate::statement_to_
    /// expression`) — the actual specification for this module: a
    /// suggestion the user can't paste back in is worse than no
    /// suggestion. Asserts every statement in a (possibly multi-statement,
    /// `;`-joined) suggestion translates successfully.
    fn assert_round_trips(sql: &str) {
        let prelexed = super::super::prelex::prelex(sql)
            .unwrap_or_else(|e| panic!("prelex failed for {sql:?}: {e}"));
        let stmts =
            sqlparser::parser::Parser::parse_sql(&sqlparser::dialect::DuckDbDialect {}, &prelexed)
                .unwrap_or_else(|e| {
                    panic!("sql parse failed for {sql:?} (prelexed: {prelexed:?}): {e}")
                });
        assert!(!stmts.is_empty(), "no statements parsed from {sql:?}");
        for stmt in &stmts {
            super::super::translate::statement_to_expression(stmt)
                .unwrap_or_else(|e| panic!("translate failed for {sql:?}: {e}"));
        }
    }

    /// The complement of `assert_round_trips`, for the "no EQL 2
    /// equivalent" messages: asserts `text` does **not** parse as SQL at
    /// all (via the same `prelex` + `sqlparser` pipeline), so a user who
    /// pastes it verbatim gets an immediate, obvious "this isn't SQL"
    /// failure rather than something that looks like a runnable query.
    fn assert_not_valid_sql(text: &str) {
        let prelexed =
            super::super::prelex::prelex(text).unwrap_or_else(|e| panic!("{text:?}: {e}"));
        assert!(
            sqlparser::parser::Parser::parse_sql(&sqlparser::dialect::DuckDbDialect {}, &prelexed)
                .is_err(),
            "expected {text:?} to NOT parse as SQL, but it did"
        );
    }

    /// Unwraps `Rendered::Query`, panicking (naming the message) if
    /// `render_account`/`render_block` ever returned `NoEquivalent` —
    /// which neither can today, but this keeps the panic message useful
    /// if that ever changes.
    fn as_query(rendered: Rendered) -> (&'static str, String, Vec<String>) {
        match rendered {
            Rendered::Query {
                table,
                fields,
                conditions,
            } => (table, fields, conditions),
            Rendered::NoEquivalent(message) => panic!("expected a Query, got: {message}"),
        }
    }

    /// Returns the bare `suggestion` text (not the `"EQL 2 uses SQL
    /// syntax..."`-wrapped `Display` of the whole error) — the part that's
    /// actually meant to parse as SQL, and the only part `assert_round_
    /// trips` should ever be handed.
    fn suggestion(source: &str) -> String {
        match legacy_error(source).unwrap() {
            EqlSqlError::LegacySyntax { suggestion } => suggestion,
            other => panic!("expected LegacySyntax, got {other:?}"),
        }
    }

    #[test]
    fn error_display_wraps_the_suggestion_with_the_expected_preamble() {
        let full = legacy_error("GET nonce FROM account vitalik.eth ON eth")
            .unwrap()
            .to_string();
        assert!(
            full.starts_with("EQL 2 uses SQL syntax. Equivalent:\n\n"),
            "{full}"
        );
        assert!(full.contains("SELECT nonce FROM accounts"), "{full}");
    }

    // --- Brief's tests -----------------------------------------------

    #[test]
    fn non_get_sources_pass_through() {
        assert!(legacy_error("SELECT 1").is_none());
        assert!(legacy_error("  copy (select 1) to 'x.json'").is_none());
    }

    #[test]
    fn account_query_gets_a_suggestion() {
        let err = suggestion("GET nonce, balance FROM account vitalik.eth ON eth");
        assert!(err.contains("SELECT nonce, balance FROM accounts"), "{err}");
        assert!(err.contains("WHERE address = vitalik.eth"), "{err}");
        assert!(err.contains("chain = eth"), "{err}");
        assert_round_trips(&err);
    }

    #[test]
    fn block_range_becomes_between() {
        let err = suggestion("GET * FROM block 1:100 ON eth");
        assert!(err.contains("SELECT * FROM blocks"), "{err}");
        assert!(err.contains("number BETWEEN 1 AND 100"), "{err}");
        assert_round_trips(&err);
    }

    #[test]
    fn tx_and_dump_render() {
        let err = suggestion(
            "GET from, to FROM tx 0x6f93d4add2ef6cdfbb9f25b9895830d719dd8edf6637b639d5c33e808ded4247 ON eth >> txs.csv",
        );
        assert!(err.contains("from_address, to_address"), "{err}");
        assert!(err.contains("COPY ("), "{err}");
        assert!(err.contains("TO 'txs.csv'"), "{err}");
        assert_round_trips(&err);
    }

    #[test]
    fn unparseable_get_still_errors_helpfully() {
        let err = suggestion("GET gibberish !!!");
        assert!(
            err.contains("docs/query.md") || err.contains("SQL"),
            "{err}"
        );
    }

    // --- Round-trip coverage for every shape the pest parser can build -

    #[test]
    fn account_wildcard_fields_round_trip() {
        // `address` is never a nameable legacy `account_field` (it's
        // implicit from `FROM account <id>` in the old grammar), so a
        // hand-typed field list can never reach all 5 `AccountField`
        // variants — only the real `*` token can. Naming the other 4 by
        // hand must stay spelled out, never collapse.
        let err = suggestion("GET nonce, balance, code, chain FROM account vitalik.eth ON eth");
        assert!(!err.contains('*'), "{err}");
        assert_round_trips(&err);

        // The actual wildcard token round-trips to `*`.
        let err = suggestion("GET * FROM account vitalik.eth ON eth");
        assert!(err.contains("SELECT * FROM accounts"), "{err}");
        assert_round_trips(&err);
    }

    #[test]
    fn block_every_field_in_declaration_order_collapses_to_wildcard() {
        // Every legacy `block_field` token names a real `BlockField`
        // variant (unlike accounts' `address`), so this is the one entity
        // where a hand-typed field list can legitimately reach the full
        // set — a real test of the wildcard-collapse equality check, not
        // just of the literal `*` token.
        let err = suggestion(
            "GET number, timestamp, size, hash, parent_hash, state_root, transactions_root, \
             receipts_root, logs_bloom, extra_data, mix_hash, total_difficulty, \
             base_fee_per_gas, withdrawals_root, blob_gas_used, excess_blob_gas, \
             parent_beacon_block_root, chain FROM block 1 ON eth",
        );
        assert!(err.contains("SELECT * FROM blocks"), "{err}");
        assert_round_trips(&err);
    }

    #[test]
    fn block_every_field_out_of_order_does_not_collapse_to_wildcard() {
        // Same *set* of fields as the wildcard, different order: this is
        // not the same request as `SELECT *` (which fixes the output
        // column order), so it must stay spelled out verbatim rather than
        // being silently rewritten to something that changes the column
        // order the user asked for.
        let err = suggestion(
            "GET chain, parent_beacon_block_root, excess_blob_gas, blob_gas_used, \
             withdrawals_root, base_fee_per_gas, total_difficulty, mix_hash, extra_data, \
             logs_bloom, receipts_root, transactions_root, state_root, parent_hash, hash, \
             size, timestamp, number FROM block 1 ON eth",
        );
        assert!(
            err.contains(
                "SELECT chain, parent_beacon_block_root, excess_blob_gas, blob_gas_used, \
                 withdrawals_root, base_fee_per_gas, total_difficulty, mix_hash, extra_data, \
                 logs_bloom, receipts_root, transactions_root, state_root, parent_hash, hash, \
                 size, timestamp, number FROM blocks"
            ),
            "{err}"
        );
        assert!(!err.contains('*'), "{err}");
        assert_round_trips(&err);
    }

    #[test]
    fn account_multiple_ids_become_in_list() {
        let err = suggestion("GET nonce FROM account vitalik.eth, ian.eth ON eth");
        assert!(err.contains("address IN (vitalik.eth, ian.eth)"), "{err}");
        assert_round_trips(&err);
    }

    #[test]
    fn account_address_id_round_trips() {
        let err =
            suggestion("GET nonce FROM account 0x1234567890123456789012345678901234567890 ON eth");
        assert_round_trips(&err);
    }

    #[test]
    fn block_number_list_round_trips() {
        let err = suggestion("GET timestamp FROM block 1,2,3 ON eth");
        assert_round_trips(&err);
    }

    #[test]
    fn block_mixed_number_and_range_round_trips() {
        let err = suggestion("GET timestamp FROM block 1,2:5,10 ON eth");
        assert!(err.contains("number = 1"), "{err}");
        assert!(err.contains("number BETWEEN 2 AND 5"), "{err}");
        assert!(err.contains("number = 10"), "{err}");
        assert_round_trips(&err);
    }

    #[test]
    fn block_tag_round_trips() {
        for tag in ["latest", "earliest", "pending", "finalized", "safe"] {
            let err = suggestion(&format!("GET number FROM block {tag} ON eth"));
            assert!(err.contains(&format!("number = {tag}")), "{tag}: {err}");
            assert_round_trips(&err);
        }
    }

    #[test]
    fn block_where_filter_range_round_trips() {
        // Unreachable via any legacy *source string* today: pest's grammar
        // matches `WHERE block = 100` fine (`blockrange_filter` is a real,
        // ordinary alternative in `block_get`'s production), but
        // `BlockFilter::try_from` (block.rs) then does
        // `value.as_str().trim_start_matches("block ").trim()` — it strips
        // the literal "block " prefix but never strips the `=` that
        // follows, so `parse_block_number_or_tag` is always handed a
        // leftover "= 100" and fails. Confirmed by running the pest parser
        // directly against "block = 100", "block=100", "block =100", and
        // "block= 100": every spacing variant errors before `Block::
        // try_from` ever returns a value with `filter: Some(...)`. This
        // corrects an imprecise claim in this task's original report,
        // which said the block WHERE-filter syntax "fails to parse" the
        // same way accounts' does — the *grammar* parses it; it's this
        // separate, pre-existing Rust-level trim bug in `block.rs`
        // (unrelated to this task, not fixed here) that breaks it before
        // `render` ever sees it.
        //
        // `render_block` still has to handle `Block.filter` being
        // populated correctly: that trim bug is a one-line difference from
        // `TransactionFilter::try_from`'s equivalent code (which does
        // strip the `=`), so it's a plausible near-term fix, at which
        // point this becomes reachable. A `Block`/`GetExpression` built
        // directly (bypassing pest) is the only way to exercise it today.
        let get = GetExpression {
            entity: Entity::Block(Block::new(
                Some(vec![]),
                Some(vec![BlockFilter::Range(BlockRange::new(
                    BlockNumberOrTag::Number(100),
                    None,
                ))]),
                vec![BlockField::Number],
            )),
            chains: vec![ChainOrRpc::Chain(Chain::Ethereum)],
            dump: None,
            limit: None,
            aliases: None,
        };
        let sql = render(&get);
        assert!(sql.contains("number = 100"), "{sql}");
        assert_round_trips(&sql);

        let get = GetExpression {
            entity: Entity::Block(Block::new(
                Some(vec![]),
                Some(vec![BlockFilter::Range(BlockRange::new(
                    BlockNumberOrTag::Number(100),
                    Some(BlockNumberOrTag::Number(200)),
                ))]),
                vec![BlockField::Number],
            )),
            chains: vec![ChainOrRpc::Chain(Chain::Ethereum)],
            dump: None,
            limit: None,
            aliases: None,
        };
        let sql = render(&get);
        assert!(sql.contains("number BETWEEN 100 AND 200"), "{sql}");
        assert_round_trips(&sql);
    }

    #[test]
    fn tx_multiple_hashes_become_in_list() {
        let err = suggestion(
            "GET hash FROM tx 0x8a6a279a4d28dcc62bcb2f2a3214c93345c107b74f3081754e27471c50783f81, \
             0x12afe6797be838900c5632de516ab415addd026335461e9471dfdec17f3d4510 ON eth",
        );
        assert!(err.contains("hash IN ("), "{err}");
        assert_round_trips(&err);
    }

    #[test]
    fn tx_all_reachable_filter_kinds_round_trip() {
        let err = suggestion(
            "GET * FROM tx WHERE \
            block = 4638757, \
            gas_limit > 10000000, \
            gas_price < 10000000, \
            max_fee_per_blob_gas >= 10000000, \
            max_fee_per_gas <= 10000000, \
            max_priority_fee_per_gas != 10000000, \
            value = 0, \
            status = true, \
            y_parity = false, \
            from = 0x1234567890123456789012345678901234567890, \
            to = 0x1234567890123456789012345678901234567890, \
            data = 0x1234, \
            type = 2 \
            ON eth",
        );
        assert!(!err.contains("--"), "no broken comment placeholder: {err}");
        assert_round_trips(&err);
    }

    #[test]
    fn tx_duplicate_block_filter_keeps_only_the_first() {
        let err = suggestion("GET value FROM tx WHERE block = 1, block = 2 ON eth");
        assert_eq!(
            err.matches("block_number").count(),
            1,
            "only the first legacy block filter is honored: {err}"
        );
        assert!(err.contains("block_number = 1"), "{err}");
        assert_round_trips(&err);
    }

    #[test]
    fn logs_full_filter_set_round_trips() {
        let err = suggestion(
            "GET address, topic0, block_number FROM log \
             WHERE block = 4638757, \
                   address = 0xdAC17F958D2ee523a2206206994597C13D831ec7, \
                   topic0 = 0xcb8241adb0c3fdb35b70c24ce35c5eb0c17af7431c99f827d44a445ca624176a \
             ON eth",
        );
        assert_round_trips(&err);
    }

    #[test]
    fn logs_block_range_round_trips() {
        let err = suggestion("GET address FROM log WHERE block = 100:200 ON eth");
        assert!(err.contains("block_number BETWEEN 100 AND 200"), "{err}");
        assert_round_trips(&err);
    }

    #[test]
    fn logs_block_hash_and_event_signature_round_trip() {
        let err = suggestion(
            "GET address FROM log \
             WHERE block_hash = 0xedb7f4a64744594838f7d9888883ae964fcb4714f6fe5cafb574d3ed6141ad5b, \
                   event_signature = Transfer(address,address,uint256) \
             ON eth",
        );
        assert!(
            err.contains("event_signature = 'Transfer(address,address,uint256)'"),
            "{err}"
        );
        assert_round_trips(&err);
    }

    // --- Known gaps: no EQL 2 equivalent, said plainly, not papered over -

    #[test]
    fn logs_without_any_block_predicate_says_so_plainly() {
        // The legacy grammar's `log_filter_list` accepts *any* single log
        // filter (it doesn't require a block one), but
        // `sql::translate::build_logs` (Task 7) requires block_number or
        // block_hash. This is a real gap between what the old grammar
        // accepted and what the new frontend requires — not something
        // `render` can paper over without inventing a predicate the user
        // never wrote. Rather than emit SQL that parses but then fails
        // `build_logs`'s check with an error naming nothing the user
        // wrote, the suggestion says plainly that there's no equivalent
        // and names the missing predicate.
        let err = suggestion(
            "GET address FROM log WHERE address = 0x1234567890123456789012345678901234567890 ON eth",
        );
        assert!(err.to_ascii_lowercase().contains("no equivalent"), "{err}");
        assert!(
            err.contains("block_number") && err.contains("block_hash"),
            "{err}"
        );
        assert_not_valid_sql(&err);
    }

    #[test]
    fn tx_without_hash_or_block_says_so_plainly() {
        // `tx_filter_list` makes `block` just one alternative among many —
        // a query filtering only on e.g. `gas_price` parses fine — but
        // `sql::translate::build_transaction` (Task 7) requires a hash or
        // a block predicate. Same treatment as logs above: say so plainly
        // rather than emit an unrunnable-looking-runnable query.
        let err = suggestion("GET value FROM tx WHERE gas_price > 100 ON eth");
        assert!(err.to_ascii_lowercase().contains("no equivalent"), "{err}");
        assert!(
            err.contains("hash") && err.contains("block_number"),
            "{err}"
        );
        assert_not_valid_sql(&err);
    }

    #[test]
    fn no_equivalent_message_is_returned_as_is_with_no_chain_condition_appended() {
        // `render` must return the plain message directly, not treat it as
        // a partial query body and append `AND chain = eth` (or a `COPY`
        // wrapper) to it — that would turn an honest "no equivalent"
        // message back into something that looks like a query fragment.
        let get = GetExpression {
            entity: Entity::Logs(Logs::new(
                vec![LogFilter::EmitterAddress(
                    "0x1234567890123456789012345678901234567890"
                        .parse()
                        .unwrap(),
                )],
                vec![LogField::Address],
            )),
            chains: vec![ChainOrRpc::Chain(Chain::Ethereum)],
            dump: None,
            limit: None,
            aliases: None,
        };
        let sql = render(&get);
        assert!(!sql.contains("chain"), "{sql}");
        assert!(!sql.contains("SELECT"), "{sql}");
    }

    // --- Chains --------------------------------------------------------

    #[test]
    fn chain_list_becomes_in_list() {
        let err = suggestion("GET size FROM block 1 ON eth, op, arb");
        assert!(err.contains("chain IN (eth, op, arb)"), "{err}");
        assert_round_trips(&err);
    }

    #[test]
    fn chain_wildcard_collapses_to_star() {
        let err = suggestion("GET size FROM block 1 ON *");
        assert!(err.contains("chain = '*'"), "{err}");
        assert_round_trips(&err);
    }

    #[test]
    fn rpc_url_chain_round_trips() {
        let err = suggestion(
            "GET nonce, balance FROM account 0x1234567890123456789012345678901234567890 ON http://localhost:8545",
        );
        assert!(err.contains("chain = 'http://localhost:8545"), "{err}");
        assert_round_trips(&err);
    }

    // --- Multi-statement legacy programs --------------------------------

    #[test]
    fn multi_statement_program_round_trips_every_statement() {
        let err = suggestion(
            "GET nonce FROM account vitalik.eth ON eth, GET timestamp FROM block 1 ON eth",
        );
        assert_eq!(err.matches("SELECT").count(), 2, "{err}");
        assert_round_trips(&err);
    }

    // --- Prefix-detection edge cases -------------------------------------

    #[test]
    fn gets_is_not_mistaken_for_get() {
        assert!(legacy_error("GETS 1").is_none());
    }

    #[test]
    fn bare_get_with_no_trailing_space_is_not_mistaken_for_legacy() {
        assert!(legacy_error("GET").is_none());
    }

    #[test]
    fn get_inside_a_string_literal_does_not_trigger_legacy_handling() {
        assert!(legacy_error("SELECT 'GET nonce FROM account x ON eth' AS x").is_none());
    }

    #[test]
    fn leading_whitespace_before_get_is_still_recognized() {
        // Recognized *and* actually translated (not just routed to the
        // fallback branch) — leading whitespace must not confuse the pest
        // parser either, since `legacy_error` hands it the untrimmed
        // source.
        let err = suggestion("   GET nonce FROM account vitalik.eth ON eth");
        assert!(err.contains("SELECT nonce FROM accounts"), "{err}");
        assert_round_trips(&err);
    }

    #[test]
    fn lowercase_get_is_recognized_but_falls_back_to_docs() {
        // `legacy_error`'s own prefix check is case-insensitive, but the
        // pest grammar's keywords (`"GET"`, `"FROM"`, `"ON"`, ...) are
        // literal, case-sensitive string matches — a lowercase legacy
        // query is routed into the legacy handler but the pest parser then
        // fails on it, same as any other unparseable `GET` source. This is
        // a pre-existing pest grammar property (not something introduced
        // or fixed by this task), documented here rather than assumed.
        let err = suggestion("get nonce from account vitalik.eth on eth");
        assert_eq!(err, "See docs/query.md for the SQL syntax.", "{err}");
    }

    // --- Defensive: shapes the pest parser can't actually produce ------

    #[test]
    fn tag_matches_every_non_numeric_variant() {
        for (t, expected) in [
            (BlockNumberOrTag::Latest, "latest"),
            (BlockNumberOrTag::Earliest, "earliest"),
            (BlockNumberOrTag::Pending, "pending"),
            (BlockNumberOrTag::Finalized, "finalized"),
            (BlockNumberOrTag::Safe, "safe"),
        ] {
            assert_eq!(tag(&t), expected);
        }
        assert_eq!(tag(&BlockNumberOrTag::Number(42)), "42");
    }

    #[test]
    fn account_with_no_ids_does_not_panic() {
        // Unreachable via the pest parser today (see `render_account`'s
        // doc comment), but constructed directly here so `render` is
        // proven not to panic if that ever changed.
        let account = Account::new(None, None, vec![AccountField::Nonce]);
        let (table, fields, conditions) = as_query(render_account(&account));
        assert_eq!(table, "accounts");
        assert_eq!(fields, "nonce");
        assert!(conditions.is_empty());
    }

    #[test]
    fn block_with_empty_ids_does_not_panic() {
        let block = Block::new(Some(vec![]), None, vec![BlockField::Number]);
        let (table, _fields, conditions) = as_query(render_block(&block));
        assert_eq!(table, "blocks");
        assert!(conditions.is_empty());
    }
}
