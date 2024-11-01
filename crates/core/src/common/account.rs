use super::ens::NameOrAddress;
use crate::interpreter::frontend::parser::Rule;
use alloy::hex::FromHexError;
use eql_macros::EnumVariants;
use pest::iterators::{Pair, Pairs};
use serde::{Deserialize, Serialize};
use std::{fmt::Display, str::FromStr};

#[derive(thiserror::Error, Debug)]
pub enum AccountError {
    #[error("Unexpected token {0}")]
    UnexpectedToken(String),

    #[error(transparent)]
    AccountFieldError(#[from] AccountFieldError),

    #[error(transparent)]
    AccountFilterError(#[from] AccountFilterError),

    #[error(transparent)]
    FromHexError(#[from] FromHexError),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Account {
    id: Option<Vec<NameOrAddress>>,
    filter: Option<Vec<AccountFilter>>,
    fields: Vec<AccountField>,
}

impl Account {
    pub fn new(
        id: Option<Vec<NameOrAddress>>,
        filter: Option<Vec<AccountFilter>>,
        fields: Vec<AccountField>,
    ) -> Self {
        Self { id, filter, fields }
    }

    pub fn ids(&self) -> Option<&Vec<NameOrAddress>> {
        self.id.as_ref()
    }

    pub fn filter(&self) -> Option<Vec<AccountFilter>> {
        self.filter.clone()
    }

    pub fn fields(&self) -> Vec<AccountField> {
        self.fields.clone()
    }
}

impl TryFrom<Pairs<'_, Rule>> for Account {
    type Error = AccountError;

    fn try_from(pairs: Pairs<'_, Rule>) -> Result<Self, Self::Error> {
        let mut fields: Vec<AccountField> = vec![];
        let mut id: Option<Vec<NameOrAddress>> = None;
        let mut filter: Option<Vec<AccountFilter>> = None;

        for pair in pairs {
            match pair.as_rule() {
                Rule::account_fields => {
                    let inner_pairs = pair.into_inner();

                    if let Some(pair) = inner_pairs.peek() {
                        if pair.as_rule() == Rule::wildcard {
                            fields = AccountField::all_variants().to_vec();
                            continue;
                        }
                    }

                    fields = inner_pairs
                        .map(|pair| AccountField::try_from(pair))
                        .collect::<Result<Vec<AccountField>, AccountFieldError>>()?;
                }
                Rule::account_id => {
                    if let Some(id) = id.as_mut() {
                        id.push(NameOrAddress::from_str(pair.as_str())?);
                    } else {
                        id = Some(vec![NameOrAddress::from_str(pair.as_str())?]);
                    }
                }
                Rule::account_filter_list => {
                    filter = Some(
                        pair.into_inner()
                            .map(|pair| AccountFilter::try_from(pair))
                            .collect::<Result<Vec<AccountFilter>, AccountFilterError>>()?,
                    );
                }
                _ => {
                    return Err(AccountError::UnexpectedToken(pair.as_str().to_string()));
                }
            }
        }

        Ok(Account { id, filter, fields })
    }
}

#[derive(thiserror::Error, Debug)]
pub enum AccountFilterError {
    #[error("Unexpected token {0} for account filter")]
    UnexpectedToken(String),

    #[error(transparent)]
    FromHexError(#[from] FromHexError),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum AccountFilter {
    Address(NameOrAddress),
}

impl TryFrom<Pair<'_, Rule>> for AccountFilter {
    type Error = AccountFilterError;

    fn try_from(pair: Pair<'_, Rule>) -> Result<Self, Self::Error> {
        match pair.as_rule() {
            Rule::address_filter => {
                let address = NameOrAddress::from_str(pair.as_str())?;
                Ok(AccountFilter::Address(address))
            }
            _ => {
                return Err(AccountFilterError::UnexpectedToken(
                    pair.as_str().to_string(),
                ));
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize, EnumVariants)]
pub enum AccountField {
    Address,
    Nonce,
    Balance,
    Code,
    Chain,
}

impl Display for AccountField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AccountField::Address => write!(f, "address"),
            AccountField::Nonce => write!(f, "nonce"),
            AccountField::Balance => write!(f, "balance"),
            AccountField::Code => write!(f, "code"),
            AccountField::Chain => write!(f, "chain"),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum AccountFieldError {
    #[error("Invalid field for entity Account: {0}")]
    InvalidField(String),

    #[error(transparent)]
    FromHexError(#[from] FromHexError),
}

impl<'a> TryFrom<Pair<'a, Rule>> for AccountField {
    type Error = AccountFieldError;

    fn try_from(pair: Pair<'a, Rule>) -> Result<Self, Self::Error> {
        AccountField::try_from(pair.as_str())
    }
}

impl TryFrom<&str> for AccountField {
    type Error = AccountFieldError;

    fn try_from(value: &str) -> Result<Self, AccountFieldError> {
        match value {
            "address" => Ok(AccountField::Address),
            "nonce" => Ok(AccountField::Nonce),
            "balance" => Ok(AccountField::Balance),
            "code" => Ok(AccountField::Code),
            "chain" => Ok(AccountField::Chain),
            invalid_field => Err(AccountFieldError::InvalidField(invalid_field.to_string())),
        }
    }
}
