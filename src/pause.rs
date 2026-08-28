use crate::storage_types::DataKey;
use soroban_sdk::{Address, Env};

pub fn require_not_paused(e: &Env) {
    if e.storage()
        .persistent()
        .get::<_, bool>(&DataKey::Paused)
        .unwrap_or(false)
    {
        panic!("ContractPaused: contract is paused");
    }
}

pub fn set_paused(e: &Env, caller: &Address, paused: bool) {
    crate::admin::check_admin(e, caller);
    e.storage().persistent().set(&DataKey::Paused, &paused);
    if paused {
        e.storage()
            .persistent()
            .set(&DataKey::PausedAtLedger, &e.ledger().sequence());
    } else {
        e.storage().persistent().remove(&DataKey::PausedAtLedger);
    }
}

/// #739: Return how many ledgers the contract has been continuously paused for.
pub fn contract_paused_for(e: &Env) -> Option<u32> {
    let paused: bool = e
        .storage()
        .persistent()
        .get::<_, bool>(&DataKey::Paused)
        .unwrap_or(false);
    if !paused {
        return None;
    }
    let paused_at: u32 = e
        .storage()
        .persistent()
        .get(&DataKey::PausedAtLedger)
        .expect("paused_at_ledger not set when paused");
    Some(e.ledger().sequence().saturating_sub(paused_at))
}
