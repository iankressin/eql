use std::{collections::HashMap, env, error::Error};

use alloy::transports::http::reqwest::Url;
use serde::{Deserialize, Serialize};

use super::chain::Chain;

const CONFIG_FILE: &str = "eql-config.json";

#[derive(thiserror::Error, Debug)]
enum ConfigErrors {
    #[error("Default RPC for chain {0} not found in config file")]
    DefaultRpcNotFound(String),
    #[error("RPC list for chain {0} not found in config file")]
    RpcNotFound(String),
}

#[derive(Serialize, Deserialize, Debug)]
struct ConfigFile {
    chains: HashMap<String, ChainConfig>,
}

#[derive(Serialize, Deserialize, Debug)]
struct ChainConfig {
    default: String,
    rpcs: Vec<String>,
}

pub struct Config {
    file_path: String,
}

impl Config {
    pub fn new() -> Self {
        if let Some(home) = env::var_os("HOME") {
            let home = home.to_string_lossy();

            Config {
                file_path: format!("{}/{}", home, CONFIG_FILE),
            }
        } else {
            panic!("Unable to get default config file path");
        }
    }

    pub fn get_chain_default_rpc(&self, chain: &Chain) -> Result<Url, Box<dyn Error>> {
        let file = std::fs::read_to_string(&self.file_path).expect("Unable to read config file");
        let config_file: ConfigFile =
            serde_json::from_str(&file).expect("Unable to parse config file");

        config_file
            .chains
            .get::<String>(&chain.into())
            // TODO: remove this unwrap
            .map(|c| c.default.parse::<Url>().unwrap())
            .ok_or_else(|| {
                Box::new(ConfigErrors::DefaultRpcNotFound(chain.into())) as Box<dyn Error>
            })
    }

    pub fn get_chain_rpcs(&self, chain: &Chain) -> Result<Vec<Url>, Box<dyn Error>> {
        let file = std::fs::read_to_string(&self.file_path).expect("Unable to read config file");
        let config_file: ConfigFile =
            serde_json::from_str(&file).expect("Unable to parse config file");

        config_file
            .chains
            .get::<String>(&chain.into())
            .map(|c| {
                c.rpcs
                    .iter()
                    // TODO: remove this unwrap
                    .map(|rpc| rpc.parse::<Url>().unwrap())
                    .collect()
            })
            .ok_or_else(|| Box::new(ConfigErrors::RpcNotFound(chain.into())) as Box<dyn Error>)
    }
}
