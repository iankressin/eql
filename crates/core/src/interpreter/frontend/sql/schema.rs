use super::EqlSqlError;
use crate::common::{
    account::AccountField, block::BlockField, logs::LogField, transaction::TransactionField,
};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum EntityKind {
    Accounts,
    Blocks,
    Transactions,
    Logs,
}

pub fn resolve_entity(name: &str) -> Result<EntityKind, EqlSqlError> {
    match name.to_ascii_lowercase().as_str() {
        "accounts" => Ok(EntityKind::Accounts),
        "blocks" => Ok(EntityKind::Blocks),
        "transactions" | "tx" => Ok(EntityKind::Transactions),
        "logs" => Ok(EntityKind::Logs),
        "account" => Err(unknown_entity(name, "accounts")),
        "block" => Err(unknown_entity(name, "blocks")),
        "transaction" | "txs" => Err(unknown_entity(name, "transactions")),
        "log" => Err(unknown_entity(name, "logs")),
        _ => Err(EqlSqlError::Validation(format!(
            "unknown entity '{name}'; expected accounts, blocks, transactions (tx) or logs"
        ))),
    }
}

fn unknown_entity(got: &str, want: &str) -> EqlSqlError {
    EqlSqlError::Validation(format!("unknown entity '{got}'; did you mean '{want}'?"))
}

// Field name resolvers below delegate to each field enum's own
// `TryFrom<&str>` impl rather than duplicating its match arms here. The
// enum owns the authoritative name<->variant mapping (and its `Display`);
// this module only lower-cases the input (so SQL identifiers are
// case-insensitive) and translates the resulting error into an
// `EqlSqlError` with the wording this frontend expects.

pub fn resolve_account_field(name: &str) -> Result<AccountField, EqlSqlError> {
    AccountField::try_from(name.to_ascii_lowercase().as_str())
        .map_err(|_| unknown_field("accounts", name))
}

pub fn resolve_block_field(name: &str) -> Result<BlockField, EqlSqlError> {
    BlockField::try_from(name.to_ascii_lowercase().as_str())
        .map_err(|_| unknown_field("blocks", name))
}

pub fn resolve_transaction_field(name: &str) -> Result<TransactionField, EqlSqlError> {
    TransactionField::try_from(name.to_ascii_lowercase().as_str())
        .map_err(|_| unknown_field("transactions", name))
}

pub fn resolve_log_field(name: &str) -> Result<LogField, EqlSqlError> {
    LogField::try_from(name.to_ascii_lowercase().as_str()).map_err(|_| unknown_field("logs", name))
}

fn unknown_field(entity: &str, field: &str) -> EqlSqlError {
    EqlSqlError::Validation(format!("unknown field '{field}' on {entity}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_entities_and_aliases() {
        assert_eq!(resolve_entity("accounts").unwrap(), EntityKind::Accounts);
        assert_eq!(
            resolve_entity("TRANSACTIONS").unwrap(),
            EntityKind::Transactions
        );
        assert_eq!(resolve_entity("tx").unwrap(), EntityKind::Transactions);
        assert_eq!(resolve_entity("logs").unwrap(), EntityKind::Logs);
        assert_eq!(resolve_entity("blocks").unwrap(), EntityKind::Blocks);
    }

    #[test]
    fn singular_names_get_a_hint() {
        let err = resolve_entity("account").unwrap_err().to_string();
        assert!(err.contains("accounts"), "hint missing: {err}");
    }

    #[test]
    fn resolves_fields_with_aliases() {
        use crate::common::transaction::TransactionField;
        assert_eq!(
            resolve_transaction_field("from_address").unwrap(),
            TransactionField::From
        );
        assert_eq!(
            resolve_transaction_field("from").unwrap(),
            TransactionField::From
        ); // quoted "from"
        assert_eq!(
            resolve_transaction_field("block_number").unwrap(),
            TransactionField::BlockNumber
        );
        assert!(resolve_transaction_field("bogus").is_err());
    }

    #[test]
    fn resolves_every_account_field_by_its_display_name() {
        for field in AccountField::all_variants() {
            assert_eq!(&resolve_account_field(&field.to_string()).unwrap(), field);
        }
    }

    #[test]
    fn resolves_every_block_field_by_its_display_name() {
        for field in BlockField::all_variants() {
            assert_eq!(&resolve_block_field(&field.to_string()).unwrap(), field);
        }
    }

    #[test]
    fn resolves_every_transaction_field_by_its_display_name() {
        for field in TransactionField::all_variants() {
            assert_eq!(
                &resolve_transaction_field(&field.to_string()).unwrap(),
                field
            );
        }
    }

    #[test]
    fn resolves_every_log_field_by_its_display_name() {
        for field in LogField::all_variants() {
            assert_eq!(&resolve_log_field(&field.to_string()).unwrap(), field);
        }
    }
}
