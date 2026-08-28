use crate::admin;
use crate::storage_types::DataKey;
use soroban_sdk::{Address, Env};

pub fn freeze_account(env: &Env, admin: &Address, account_id: &Address) {
    admin::check_admin(env, admin);
    let stored_admin: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .expect("admin not set");
    if account_id == &stored_admin {
        panic!("InvalidFreeze: cannot freeze the admin address");
    }
    let is_frozen: bool = env
        .storage()
        .persistent()
        .get(&DataKey::Frozen(account_id.clone()))
        .unwrap_or(false);
    if is_frozen {
        panic!("AlreadyFrozen: account is already frozen");
    }
    env.storage()
        .persistent()
        .set(&DataKey::Frozen(account_id.clone()), &true);
}

/// #743: freeze an account until a specific ledger. The freeze auto-clears once
/// the current ledger passes `until_ledger`.
pub fn freeze_until(env: &Env, admin: &Address, account_id: &Address, until_ledger: u32) {
    admin::check_admin(env, admin);
    if until_ledger <= env.ledger().sequence() {
        panic!("InvalidFreezeUntil: until_ledger must be in the future");
    }
    env.storage()
        .persistent()
        .set(&DataKey::FrozenUntil(account_id.clone()), &until_ledger);
}

pub fn unfreeze_account(env: &Env, _admin: &Address, account_id: &Address) {
    let is_frozen: bool = env
        .storage()
        .persistent()
        .get(&DataKey::Frozen(account_id.clone()))
        .unwrap_or(false);
    let has_timed_freeze: bool = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKey::FrozenUntil(account_id.clone()))
        .is_some();
    if !is_frozen && !has_timed_freeze {
        panic!("NotFrozen: account is not frozen");
    }
    env.storage()
        .persistent()
        .remove(&DataKey::Frozen(account_id.clone()));
    env.storage()
        .persistent()
        .remove(&DataKey::FrozenUntil(account_id.clone()));
}

pub fn is_frozen(env: &Env, account_id: &Address) -> bool {
    // #743: a timed freeze auto-clears once the current ledger passes until_ledger
    if let Some(until) = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKey::FrozenUntil(account_id.clone()))
    {
        if env.ledger().sequence() >= until {
            env.storage()
                .persistent()
                .remove(&DataKey::FrozenUntil(account_id.clone()));
            return false;
        }
        return true;
    }
    env.storage()
        .persistent()
        .get(&DataKey::Frozen(account_id.clone()))
        .unwrap_or(false)
}
