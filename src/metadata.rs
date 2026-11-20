use crate::storage_types::DataKey;
use soroban_sdk::{contracttype, Env, String};

/// Name stored when the deployer does not supply one.
pub const DEFAULT_NAME: &str = "VeriTix";

/// Symbol stored when the deployer does not supply one.
pub const DEFAULT_SYMBOL: &str = "VTX";

/// Decimals stored when the deployer does not supply one.
pub const DEFAULT_DECIMALS: u32 = 7;

/// The three SEP-41 metadata fields, as stored at initialization.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenMetadata {
    pub name: String,
    pub symbol: String,
    pub decimals: u32,
}

/// The metadata a contract falls back on when nothing has been stored yet.
pub fn defaults(e: &Env) -> TokenMetadata {
    TokenMetadata {
        name: String::from_str(e, DEFAULT_NAME),
        symbol: String::from_str(e, DEFAULT_SYMBOL),
        decimals: DEFAULT_DECIMALS,
    }
}

/// Writes all three metadata fields, overwriting whatever was there.
pub fn store(e: &Env, metadata: &TokenMetadata) {
    e.storage().persistent().set(&DataKey::Name, &metadata.name);
    e.storage()
        .persistent()
        .set(&DataKey::Symbol, &metadata.symbol);
    e.storage()
        .persistent()
        .set(&DataKey::Decimals, &metadata.decimals);
}

/// Reads the stored metadata, filling in defaults for any field that is unset.
///
/// Metadata is written once at initialization and never mutated afterwards, so
/// the per-field fallbacks only ever fire on a contract that was deployed
/// without a complete metadata record.
pub fn load(e: &Env) -> TokenMetadata {
    let fallback = defaults(e);
    TokenMetadata {
        name: e
            .storage()
            .persistent()
            .get(&DataKey::Name)
            .unwrap_or(fallback.name),
        symbol: e
            .storage()
            .persistent()
            .get(&DataKey::Symbol)
            .unwrap_or(fallback.symbol),
        decimals: e
            .storage()
            .persistent()
            .get(&DataKey::Decimals)
            .unwrap_or(fallback.decimals),
    }
}
