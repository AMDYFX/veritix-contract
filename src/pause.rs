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
}
