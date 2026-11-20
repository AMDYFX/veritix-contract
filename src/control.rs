use crate::storage_types::DataKey;
use soroban_sdk::{Address, Env};

/// Whether the whole contract is currently paused.
///
/// The pause flag is contract-wide configuration, so it lives in instance
/// storage alongside the admin and the supply counters (see `docs/ttl.md`).
pub fn is_paused(e: &Env) -> bool {
    e.storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
}

/// Panics while the contract is paused.
///
/// Pausing is the blunt compliance control, so it blocks every path that moves
/// tokens rather than any single entry point — a pause that a future code path
/// could forget to check would be worse than no pause at all.
pub fn require_not_paused(e: &Env) {
    if is_paused(e) {
        panic!("Paused: token transfers are suspended");
    }
}

/// Pauses or resumes the contract.
pub fn set_paused(e: &Env, paused: bool) {
    if paused {
        e.storage().instance().set(&DataKey::Paused, &true);
    } else {
        e.storage().instance().remove(&DataKey::Paused);
    }
}

/// Whether `account` is frozen and cannot move tokens.
pub fn is_frozen(e: &Env, account: &Address) -> bool {
    e.storage()
        .persistent()
        .get(&DataKey::Frozen(account.clone()))
        .unwrap_or(false)
}

/// Panics when `account` is frozen.
pub fn require_not_frozen(e: &Env, account: &Address) {
    if is_frozen(e, account) {
        panic!("Frozen: account is frozen");
    }
}

/// Freezes or thaws `account`.
///
/// A frozen account keeps its balance: freezing blocks movement, it is not a
/// clawback, so the tokens stay in total supply and stay visible to the owner.
pub fn set_frozen(e: &Env, account: &Address, frozen: bool) {
    if frozen {
        e.storage()
            .persistent()
            .set(&DataKey::Frozen(account.clone()), &true);
    } else {
        e.storage()
            .persistent()
            .remove(&DataKey::Frozen(account.clone()));
    }
}
