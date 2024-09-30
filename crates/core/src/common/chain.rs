use std::error::Error;

use alloy::transports::http::reqwest::Url;

use super::config::Config;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Chain {
    Ethereum,
    Sepolia,
    Arbitrum,
    Base,
    Blast,
    Optimism,
    Polygon,
    Anvil,
    Mantle,
    Zksync,
    Taiko,
    Celo,
    Avalanache,
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
}

impl Default for Chain {
    fn default() -> Self {
        Chain::Ethereum
    }
}

impl TryFrom<&str> for Chain {
    type Error = &'static str;

    fn try_from(chain: &str) -> Result<Self, Self::Error> {
        match chain {
            "eth" => Ok(Chain::Ethereum),
            "sepolia" => Ok(Chain::Sepolia),
            "arb" => Ok(Chain::Arbitrum),
            "base" => Ok(Chain::Base),
            "blast" => Ok(Chain::Blast),
            "optimism" => Ok(Chain::Optimism),
            "polygon" => Ok(Chain::Polygon),
            "anvil" => Ok(Chain::Anvil),
            "mantle" => Ok(Chain::Mantle),
            "zksync" => Ok(Chain::Zksync),
            "taiko" => Ok(Chain::Taiko),
            "celo" => Ok(Chain::Celo),
            "avalanche" => Ok(Chain::Avalanache),
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
            _ => Err("Invalid chain"),
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
            Chain::Anvil => 31337,
            Chain::Mantle => 5000,
            Chain::Zksync => 324,
            Chain::Taiko => 167000,
            Chain::Celo => 42220,
            Chain::Avalanache => 43114,
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
        }
    }
}

impl From<&Chain> for String {
    fn from(value: &Chain) -> Self {
        match value {
            Chain::Ethereum => "eth".to_string(),
            Chain::Sepolia => "sepolia".to_string(),
            Chain::Arbitrum => "arb".to_string(),
            Chain::Base => "base".to_string(),
            Chain::Blast => "blast".to_string(),
            Chain::Optimism => "optimism".to_string(),
            Chain::Polygon => "polygon".to_string(),
            Chain::Anvil => "anvil".to_string(),
            Chain::Mantle => "mantle".to_string(),
            Chain::Zksync => "zksync".to_string(),
            Chain::Taiko => "taiko".to_string(),
            Chain::Celo => "celo".to_string(),
            Chain::Avalanache => "avalanche".to_string(),
            Chain::Scroll => "scroll".to_string(),
            Chain::Bnb => "bnb".to_string(),
            Chain::Linea => "linea".to_string(),
            Chain::Zora => "zora".to_string(),
            Chain::Moonbeam => "moonbeam".to_string(),
            Chain::Moonriver => "moonriver".to_string(),
            Chain::Ronin => "ronin".to_string(),
            Chain::Fantom => "fantom".to_string(),
            Chain::Kava => "kava".to_string(),
            Chain::Gnosis => "gnosis".to_string(),
        }
    }
}

impl Chain {
    pub fn rpc_url(&self) -> Result<Url, Box<dyn Error>> {
        Config::new().get_chain_default_rpc(self)
    }
}
