use crate::interpreter::frontend::parser::Rule;

use super::config::Config;
use alloy::{
    providers::{Provider, ProviderBuilder},
    transports::http::reqwest::Url,
};
use anyhow::Result;
use core::fmt;
use eql_macros::EnumVariants;
use pest::iterators::Pairs;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ChainOrRpc {
    Chain(Chain),
    Rpc(Url),
}

impl ChainOrRpc {
    pub fn rpc_url(&self) -> Result<Url> {
        match self {
            ChainOrRpc::Chain(chain) => Ok(chain.rpc_url()?.clone()),
            ChainOrRpc::Rpc(url) => Ok(url.clone()),
        }
    }

    pub async fn to_chain(&self) -> Result<Chain> {
        match self {
            ChainOrRpc::Chain(chain) => Ok(chain.clone()),
            ChainOrRpc::Rpc(rpc) => {
                let provider = ProviderBuilder::new().on_http(rpc.clone());
                let chain_id = provider.get_chain_id().await?;
                let chain = chain_id.try_into()?;
                Ok(chain)
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, EnumVariants, Serialize, Deserialize)]
pub enum Chain {
    Ethereum,
    Sepolia,
    Arbitrum,
    Base,
    Blast,
    Optimism,
    Polygon,
    Mantle,
    Zksync,
    Taiko,
    Celo,
    Avalanche,
    Scroll,
    Bnb,
    Linea,
    Zora,
    Moonbeam,
    Moonriver,
    Ronin,
    Fantom,
    Kava,
    Gnosis,

    // Short-lived Pectra testnet
    Mekong,
}

#[derive(thiserror::Error, Debug)]
pub enum ChainError {
    #[error("Invalid chain {0}")]
    InvalidChain(String),
}

impl TryFrom<Pairs<'_, Rule>> for Chain {
    type Error = ChainError;

    fn try_from(pairs: Pairs<'_, Rule>) -> Result<Self, Self::Error> {
        for pair in pairs {
            match pair.as_rule() {
                Rule::chain => return Ok(Chain::try_from(pair.as_str())?),
                _ => return Err(ChainError::InvalidChain(pair.as_str().to_string())),
            }
        }
        Ok(Chain::default())
    }
}

impl Chain {
    pub fn from_selector(selector: &str) -> Result<Vec<ChainOrRpc>, ChainError> {
        if selector == "*" {
            let chains = Chain::all_variants();
            let chains = chains
                .into_iter()
                .map(|chain| ChainOrRpc::Chain(chain.clone()))
                .collect::<Vec<ChainOrRpc>>();
            Ok(chains)
        } else {
            // Parse comma-separated chain list
            let chains = selector
                .split(',')
                .map(str::trim)
                .map(|s| Chain::try_from(s).map(ChainOrRpc::Chain))
                .collect::<Result<Vec<ChainOrRpc>, ChainError>>()?;

            Ok(chains)
        }
    }

    pub fn rpc_url(&self) -> Result<Url> {
        match Config::new().get_chain_default_rpc(self) {
            Ok(Some(url)) => Ok(url),
            Ok(None) => Ok(self.rpc_fallback().parse()?),
            Err(e) => Err(e),
        }
    }

    fn rpc_fallback(&self) -> &str {
        match self {
            Chain::Ethereum => "https://ethereum.drpc.org",
            Chain::Sepolia => "https://rpc.ankr.com/eth_sepolia",
            Chain::Arbitrum => "https://rpc.ankr.com/arbitrum",
            Chain::Base => "https://rpc.ankr.com/base",
            Chain::Blast => "https://rpc.ankr.com/blast",
            Chain::Optimism => "https://optimism.drpc.org",
            Chain::Polygon => "https://polygon.llamarpc.com",
            Chain::Mantle => "https://mantle.drpc.org",
            Chain::Zksync => "https://mainnet.era.zksync.io",
            Chain::Taiko => "https://rpc.taiko.xyz",
            Chain::Celo => "https://1rpc.io/celo",
            Chain::Avalanche => "https://avalanche.drpc.org",
            Chain::Scroll => "https://scroll.drpc.org",
            Chain::Bnb => "https://binance.llamarpc.com",
            Chain::Linea => "https://rpc.linea.build",
            Chain::Zora => "https://zora.drpc.org",
            Chain::Moonbeam => "https://moonbeam.drpc.org",
            Chain::Moonriver => "https://moonriver.drpc.org",
            Chain::Ronin => "https://ronin.drpc.org",
            Chain::Fantom => "https://fantom.drpc.org",
            Chain::Kava => "https://evm.kava.io",
            Chain::Gnosis => "https://gnosis.drpc.org",
            Chain::Mekong => "https://rpc.mekong.ethpandaops.io",
        }
    }
}

impl Default for Chain {
    fn default() -> Self {
        Chain::Ethereum
    }
}

impl TryFrom<&str> for Chain {
    type Error = ChainError;

    fn try_from(chain: &str) -> Result<Self, Self::Error> {
        match chain {
            "eth" => Ok(Chain::Ethereum),
            "sepolia" => Ok(Chain::Sepolia),
            "arb" => Ok(Chain::Arbitrum),
            "base" => Ok(Chain::Base),
            "blast" => Ok(Chain::Blast),
            "op" => Ok(Chain::Optimism),
            "polygon" => Ok(Chain::Polygon),
            "mantle" => Ok(Chain::Mantle),
            "zksync" => Ok(Chain::Zksync),
            "taiko" => Ok(Chain::Taiko),
            "celo" => Ok(Chain::Celo),
            "avalanche" => Ok(Chain::Avalanche),
            "scroll" => Ok(Chain::Scroll),
            "bnb" => Ok(Chain::Bnb),
            "linea" => Ok(Chain::Linea),
            "zora" => Ok(Chain::Zora),
            "moonbeam" => Ok(Chain::Moonbeam),
            "moonriver" => Ok(Chain::Moonriver),
            "ronin" => Ok(Chain::Ronin),
            "fantom" => Ok(Chain::Fantom),
            "kava" => Ok(Chain::Kava),
            "gnosis" => Ok(Chain::Gnosis),
            "mekong" => Ok(Chain::Mekong),
            _ => Err(ChainError::InvalidChain(chain.to_string())),
        }
    }
}

impl From<&Chain> for u64 {
    fn from(value: &Chain) -> Self {
        match value {
            Chain::Ethereum => 1,
            Chain::Sepolia => 11155111,
            Chain::Arbitrum => 42161,
            Chain::Base => 8453,
            Chain::Blast => 238,
            Chain::Optimism => 10,
            Chain::Polygon => 137,
            Chain::Mantle => 5000,
            Chain::Zksync => 324,
            Chain::Taiko => 167000,
            Chain::Celo => 42220,
            Chain::Avalanche => 43114,
            Chain::Scroll => 534352,
            Chain::Bnb => 56,
            Chain::Linea => 59144,
            Chain::Zora => 7777777,
            Chain::Moonbeam => 1284,
            Chain::Moonriver => 1285,
            Chain::Ronin => 2020,
            Chain::Fantom => 250,
            Chain::Kava => 2222,
            Chain::Gnosis => 100,
            Chain::Mekong => 7078815900,
        }
    }
}

impl TryFrom<u64> for Chain {
    type Error = ChainError;

    fn try_from(chain_id: u64) -> Result<Self, Self::Error> {
        match chain_id {
            1 => Ok(Chain::Ethereum),
            11155111 => Ok(Chain::Sepolia),
            42161 => Ok(Chain::Arbitrum),
            8453 => Ok(Chain::Base),
            238 => Ok(Chain::Blast),
            10 => Ok(Chain::Optimism),
            137 => Ok(Chain::Polygon),
            5000 => Ok(Chain::Mantle),
            324 => Ok(Chain::Zksync),
            167000 => Ok(Chain::Taiko),
            42220 => Ok(Chain::Celo),
            43114 => Ok(Chain::Avalanche),
            534352 => Ok(Chain::Scroll),
            56 => Ok(Chain::Bnb),
            59144 => Ok(Chain::Linea),
            7777777 => Ok(Chain::Zora),
            1284 => Ok(Chain::Moonbeam),
            1285 => Ok(Chain::Moonriver),
            2020 => Ok(Chain::Ronin),
            250 => Ok(Chain::Fantom),
            2222 => Ok(Chain::Kava),
            100 => Ok(Chain::Gnosis),
            _ => Err(ChainError::InvalidChain(chain_id.to_string())),
        }
    }
}

impl fmt::Display for Chain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let chain_str = match self {
            Chain::Ethereum => "eth",
            Chain::Sepolia => "sepolia",
            Chain::Arbitrum => "arb",
            Chain::Base => "base",
            Chain::Blast => "blast",
            Chain::Optimism => "op",
            Chain::Polygon => "polygon",
            Chain::Mantle => "mantle",
            Chain::Zksync => "zksync",
            Chain::Taiko => "taiko",
            Chain::Celo => "celo",
            Chain::Avalanche => "avalanche",
            Chain::Scroll => "scroll",
            Chain::Bnb => "bnb",
            Chain::Linea => "linea",
            Chain::Zora => "zora",
            Chain::Moonbeam => "moonbeam",
            Chain::Moonriver => "moonriver",
            Chain::Ronin => "ronin",
            Chain::Fantom => "fantom",
            Chain::Kava => "kava",
            Chain::Gnosis => "gnosis",
            Chain::Mekong => "mekong",
        };
        write!(f, "{}", chain_str)
    }
}
