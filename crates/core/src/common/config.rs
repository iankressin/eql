use super::chain::Chain;
use alloy::transports::http::reqwest::Url;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

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

/// Process-wide store for `SET rpc_<chain> = '<url>'` overrides.
///
/// This is deliberately global rather than threaded through per-query state:
/// `SET` is a session statement, and EQL has no other notion of "session"
/// than the process running it, so a process-wide map is the natural home.
/// The consequence is real, though: an override set by one query changes
/// `Chain::rpc_url` for *every* later query in the process, including ones
/// unrelated to whoever called `SET` (see `Chain::rpc_url`'s doc comment).
/// It is never cleared automatically.
///
/// Because `cargo test` runs every test in this crate in one process across
/// many threads, a `SET`-equivalent call in one test is visible to any other
/// test that reads `session_rpc` for the same chain. Tests that exercise
/// this store use a chain no other test in the crate reads or writes
/// (`Chain::Sepolia`/`Chain::Gnosis` in `session_rpc_override_wins`, distinct
/// from the chain `set_rpc_translates` in `translate.rs` uses) so they can
/// run concurrently with the rest of the suite without interference.
static SESSION_RPCS: OnceLock<Mutex<HashMap<Chain, Url>>> = OnceLock::new();

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

    /// Records a session-scoped RPC override for `chain`, set by `SET
    /// rpc_<chain> = '<url>'`. See `SESSION_RPCS`'s doc comment for the
    /// process-wide blast radius this carries.
    pub fn set_session_rpc(chain: &Chain, url: Url) {
        SESSION_RPCS
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .expect("session rpc lock")
            .insert(chain.clone(), url);
    }

    /// Returns the session-scoped RPC override for `chain`, if `SET
    /// rpc_<chain>` has been called for it in this process. Consulted by
    /// `Chain::rpc_url` ahead of the on-disk config and built-in fallback.
    pub fn session_rpc(chain: &Chain) -> Option<Url> {
        SESSION_RPCS
            .get()?
            .lock()
            .expect("session rpc lock")
            .get(chain)
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_rpc_override_wins() {
        use crate::common::chain::Chain;
        use alloy::transports::http::reqwest::Url;
        let url = Url::parse("https://session-node:8545").unwrap();
        Config::set_session_rpc(&Chain::Sepolia, url.clone());
        assert_eq!(Config::session_rpc(&Chain::Sepolia), Some(url));
        assert_eq!(Config::session_rpc(&Chain::Gnosis), None);
    }

    #[test]
    fn session_rpc_defaults_to_none_before_any_set() {
        // A chain nothing in the suite ever calls `set_session_rpc` for
        // (distinct from `Sepolia`/`Gnosis` above and from `Ethereum`, which
        // `execution_engine.rs` and the `resolve_*` modules' tests rely on
        // resolving to the real fallback RPC) should read back `None`
        // regardless of test execution order.
        use crate::common::chain::Chain;
        assert_eq!(Config::session_rpc(&Chain::Kava), None);
    }

    #[test]
    fn set_session_rpc_overwrites_a_previous_override_for_the_same_chain() {
        // `SET rpc_eth` given twice in one program is last-write-wins, not
        // an error — unlike the single-slot filters elsewhere in this
        // crate (see `translate::push_block_id_filter`), `SET` is meant to
        // behave like SQL's own `SET`, where a later assignment simply
        // replaces an earlier one.
        use crate::common::chain::Chain;
        use alloy::transports::http::reqwest::Url;
        let chain = Chain::Moonriver;
        let first = Url::parse("https://first-node:8545").unwrap();
        let second = Url::parse("https://second-node:8545").unwrap();
        Config::set_session_rpc(&chain, first);
        Config::set_session_rpc(&chain, second.clone());
        assert_eq!(Config::session_rpc(&chain), Some(second));
    }
}
