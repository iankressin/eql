use crate::interpreter::frontend::parser::Rule;
use pest::iterators::Pairs;
use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct Dump {
    pub name: String,
    pub format: DumpFormat,
}

impl Dump {
    pub fn new(name: String, format: DumpFormat) -> Self {
        Self { name, format }
    }

    pub fn path(&self) -> String {
        format!("{}.{}", self.name, self.format)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DumpError {
    #[error("Invalid dump format: {0}")]
    InvalidDumpFormat(String),
    #[error("File name not found")]
    FileNameNotFound,
    #[error("File format not found")]
    FileFormatNotFound,
}

impl<'a> TryFrom<Pairs<'a, Rule>> for Dump {
    type Error = DumpError;

    fn try_from(pairs: Pairs<'a, Rule>) -> Result<Self, Self::Error> {
        let mut pairs = pairs;

        let name = pairs
            .next()
            .ok_or(DumpError::FileFormatNotFound)?
            .as_str()
            .to_string();

        let format: DumpFormat = pairs
            .next()
            .ok_or(DumpError::FileFormatNotFound)?
            .as_str()
            .try_into()?;

        Ok(Dump::new(name, format))
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum DumpFormat {
    Json,
    Csv,
    Parquet,
}

impl TryFrom<&str> for DumpFormat {
    type Error = DumpError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "json" => Ok(DumpFormat::Json),
            "csv" => Ok(DumpFormat::Csv),
            "parquet" => Ok(DumpFormat::Parquet),
            invalid_format => Err(DumpError::InvalidDumpFormat(invalid_format.to_string())),
        }
    }
}

impl Display for DumpFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DumpFormat::Json => write!(f, "json"),
            DumpFormat::Csv => write!(f, "csv"),
            DumpFormat::Parquet => write!(f, "parquet"),
        }
    }
}
