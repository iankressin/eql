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
/// It is never cleared automatically in production.
///
/// Because `cargo test` runs every test in this crate in one process across
/// many threads, a `SET`-equivalent call in one test is visible to any other
/// test that reads `session_rpc` for the same chain — including tests added
/// long after this file, in files that never import it. A doc comment alone
/// can't stop that; `SessionRpcTestGuard` below is the actual guard rail:
/// every test that touches this store goes through it, so a chain already
/// claimed panics immediately (naming the conflict) instead of silently
/// receiving the wrong override, and no override outlives the test that set
/// it.
static SESSION_RPCS: OnceLock<Mutex<HashMap<Chain, Url>>> = OnceLock::new();

/// Chains reserved for tests that exercise `SESSION_RPCS` directly (as
/// opposed to chains real RPC-hitting tests resolve against, chiefly
/// `Chain::Ethereum` — see `ChainOrRpc`/`resolve_*` test modules). Anyone
/// adding a new session-RPC test picks a chain from this list and goes
/// through `SessionRpcTestGuard`; anyone adding a real network test should
/// grep for this constant first and avoid these chains (or move one out of
/// the list if it must be reused, updating the tests that reserved it).
#[cfg(test)]
const SESSION_RPC_TEST_CHAINS: &[Chain] = &[
    Chain::Sepolia,
    Chain::Gnosis,
    Chain::Moonriver,
    Chain::Moonbeam,
    Chain::Kava,
];

/// Tracks which `SESSION_RPC_TEST_CHAINS` entries currently have a live
/// `SessionRpcTestGuard`, so a second test trying to claim the same chain
/// while the first is still running panics instead of racing it.
#[cfg(test)]
static SESSION_RPC_TEST_CLAIMS: OnceLock<Mutex<std::collections::HashSet<Chain>>> = OnceLock::new();

/// RAII guard for tests that exercise `SESSION_RPCS`. This is the
/// structural half of the fix — `SESSION_RPC_TEST_CHAINS` only documents
/// which chains are safe to use; this guard *enforces* it and cleans up
/// after itself:
///
/// - Construction panics, naming the chain, if it isn't in
///   `SESSION_RPC_TEST_CHAINS` (a new session-RPC test must consciously
///   reserve a chain, not pick one at random and hope) or if another live
///   guard already holds it (two tests concurrently touching the same
///   chain fail loudly instead of one silently clobbering the other's
///   override).
/// - `Drop` removes `chain`'s entry from `SESSION_RPCS` and releases the
///   claim, so an override set by one test can never outlive that test —
///   closing the "permanent for the rest of the process" hole a bare
///   `Config::set_session_rpc` call in a test would otherwise leave open.
#[cfg(test)]
pub(crate) struct SessionRpcTestGuard {
    chain: Chain,
}

#[cfg(test)]
impl SessionRpcTestGuard {
    /// Claims `chain` for the caller's test without setting an override —
    /// for tests that reach `Config::set_session_rpc` indirectly (e.g.
    /// through `ExecutionEngine::run`) rather than calling it themselves.
    pub(crate) fn reserve(chain: Chain) -> Self {
        assert!(
            SESSION_RPC_TEST_CHAINS.contains(&chain),
            "{chain} is not in SESSION_RPC_TEST_CHAINS (crates/core/src/common/config.rs); \
             add it there only after confirming no real RPC-hitting test resolves against it"
        );
        // The lock guard must be dropped *before* the `assert!` below: if
        // it panics while the guard is still alive, the guard's own `Drop`
        // poisons the mutex mid-unwind, and any other live
        // `SessionRpcTestGuard`'s `Drop` trying to clean up afterwards
        // would then hit a poisoned lock — a panic inside a destructor,
        // which Rust can't unwind through and aborts the whole test
        // binary instead of just failing this one test. Bounding the lock
        // to this block, and computing `newly_claimed` before asserting on
        // it, keeps the panic (if any) outside the locked section.
        let newly_claimed = {
            let mut claimed = SESSION_RPC_TEST_CLAIMS
                .get_or_init(|| Mutex::new(std::collections::HashSet::new()))
                .lock()
                .expect("session rpc test claims lock");
            claimed.insert(chain.clone())
        };
        assert!(
            newly_claimed,
            "{chain} is already claimed by another live SessionRpcTestGuard — two tests are \
             concurrently exercising the same chain's session RPC override"
        );
        Self { chain }
    }

    /// Claims `chain` and immediately sets `url` as its session override.
    pub(crate) fn acquire(chain: Chain, url: Url) -> Self {
        let guard = Self::reserve(chain.clone());
        Config::set_session_rpc(&chain, url);
        guard
    }
}

#[cfg(test)]
impl Drop for SessionRpcTestGuard {
    fn drop(&mut self) {
        // Recovers the inner data with `unwrap_or_else(PoisonError::into_inner)`
        // rather than `.expect(..)`: this cleanup runs during unwinding when
        // a `#[should_panic]` test panics inside the locked section above,
        // and a *second* panic here (from a poisoned lock) would abort the
        // whole test binary instead of just this one test failing. This
        // guard's own job is to make collisions loud without ever being the
        // thing that takes down the rest of the suite.
        if let Some(map) = SESSION_RPCS.get() {
            map.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&self.chain);
        }
        if let Some(claims) = SESSION_RPC_TEST_CLAIMS.get() {
            claims
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&self.chain);
        }
    }
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
        // `_guard` clears `Sepolia`'s override on drop, at the end of this
        // test — see `SessionRpcTestGuard`'s doc comment for why that
        // matters (without it, this override would otherwise outlive this
        // test for the rest of the process).
        let _guard = SessionRpcTestGuard::acquire(Chain::Sepolia, url.clone());
        assert_eq!(Config::session_rpc(&Chain::Sepolia), Some(url));
        assert_eq!(Config::session_rpc(&Chain::Gnosis), None);
    }

    #[test]
    fn session_rpc_defaults_to_none_before_any_set() {
        // A chain nothing in the suite currently claims (see
        // `SESSION_RPC_TEST_CHAINS`) should read back `None`. This test
        // only reads, so it doesn't need a `SessionRpcTestGuard` itself —
        // but every test that *does* mutate one of these chains must go
        // through the guard precisely so this kind of assertion stays true
        // once that test finishes, not just "usually true because nobody
        // happens to touch it".
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
        let _guard = SessionRpcTestGuard::acquire(chain.clone(), first);
        Config::set_session_rpc(&chain, second.clone());
        assert_eq!(Config::session_rpc(&chain), Some(second));
    }

    #[test]
    #[should_panic(expected = "is not in SESSION_RPC_TEST_CHAINS")]
    fn guard_panics_loudly_on_an_unreserved_chain() {
        // Locks in the "structural, not conventional" property: picking a
        // chain outside the reserved list fails the test immediately and
        // says why, instead of quietly running against (say) `Ethereum` and
        // corrupting every other test that expects its real RPC.
        let _guard = SessionRpcTestGuard::acquire(
            Chain::Ethereum,
            Url::parse("https://should-not-be-reachable").unwrap(),
        );
    }

    #[test]
    #[should_panic(expected = "is already claimed by another live SessionRpcTestGuard")]
    fn guard_panics_loudly_on_a_concurrent_claim() {
        // Locks in the collision-is-loud property directly: two guards
        // live at once for the same chain must panic naming the conflict,
        // not silently let the second one win.
        let chain = Chain::Gnosis;
        let _first = SessionRpcTestGuard::reserve(chain.clone());
        let _second = SessionRpcTestGuard::reserve(chain);
    }
}
