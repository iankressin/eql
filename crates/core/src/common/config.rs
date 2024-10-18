use super::chain::Chain;
use alloy::transports::http::reqwest::Url;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;

const CONFIG_FILE: &str = "eql-config.json";

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
    file_path: Option<PathBuf>,
}

impl Config {
    /// Create a new config instance based on the config file path
    /// The precedence of the config file is:
    /// 1. EQL_CONFIG_PATH environment variable
    /// 2. Current working directory (env::current_dir)
    /// 3. $HOME/eql-config.json
    pub fn new() -> Self {
        if let Ok(env_config_path) = env::var("EQL_CONFIG_PATH") {
            let env_config_path = PathBuf::from(&env_config_path);
            if env_config_path.exists() {
                return Config {
                    file_path: Some(env_config_path),
                };
            }
        }

        if let Ok(curr_dir) = env::current_dir() {
            let curr_dir_config_path = curr_dir.join(CONFIG_FILE);
            if curr_dir_config_path.exists() {
                return Config {
                    file_path: Some(curr_dir_config_path),
                };
            }
        }

        if let Ok(env_home) = env::var("HOME") {
            let home_config_path = PathBuf::from(env_home).join(CONFIG_FILE);
            if home_config_path.exists() {
                return Config {
                    file_path: Some(home_config_path),
                };
            }
        }

        Config { file_path: None }
    }

    pub fn get_chain_default_rpc(&self, chain: &Chain) -> Result<Option<Url>> {
        match &self.file_path {
            Some(file_path) => {
                let file = fs::read_to_string(file_path)?;
                let config_file: ConfigFile = serde_json::from_str(&file)?;

                if let Some(chain_config) = config_file.chains.get(&chain.to_string()) {
                    let url = chain_config.default.parse::<Url>()?;
                    Ok(Some(url))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    pub fn get_chain_rpcs(&self, chain: &Chain) -> Result<Option<Vec<Url>>> {
        match &self.file_path {
            Some(file_path) => {
                let file = fs::read_to_string(file_path)?;
                let config_file: ConfigFile = serde_json::from_str(&file)?;

                if let Some(chain_config) = config_file.chains.get(&chain.to_string()) {
                    let urls: Result<Vec<Url>, _> =
                        chain_config.rpcs.iter().map(|rpc| rpc.parse()).collect();
                    Ok(Some(urls?))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }
}
