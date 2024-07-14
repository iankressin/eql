#[derive(Debug, PartialEq, Eq)]
pub enum Chain {
    Ethereum,
    Arbitrum,
    Base,
    Blast,
    Optimism,
    Polygon,
    Anvil,
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
            "arb" => Ok(Chain::Arbitrum),
            "base" => Ok(Chain::Base),
            "blast" => Ok(Chain::Blast),
            "optimism" => Ok(Chain::Optimism),
            "polygon" => Ok(Chain::Polygon),
            "anvil" => Ok(Chain::Anvil),
            _ => Err("Invalid chain"),
        }
    }
}

impl From<&Chain> for u64 {
    fn from(value: &Chain) -> Self {
        match value {
            Chain::Ethereum => 1,
            Chain::Arbitrum => 42161,
            Chain::Base => 8453,
            Chain::Blast => 238,
            Chain::Optimism => 10,
            Chain::Polygon => 137,
            Chain::Anvil => 31337,
        }
    }
}

impl Chain {
    pub fn rpc_url(&self) -> &str {
        match self {
            Chain::Ethereum => "https://eth.llamarpc.com",
            Chain::Arbitrum => "https://rpc.ankr.com/arbitrum",
            Chain::Base => "https://rpc.ankr.com/base",
            Chain::Blast => "https://rpc.ankr.com/blast",
            Chain::Optimism => "https://rpc.ankr.com/optimism",
            Chain::Polygon => "https://polygon.llamarpc.com",
            Chain::Anvil => "http://localhost:8545",
        }
    }
}
