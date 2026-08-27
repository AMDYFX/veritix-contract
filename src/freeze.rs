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

pub fn unfreeze_account(env: &Env, _admin: &Address, account_id: &Address) {
    let is_frozen: bool = env
        .storage()
        .persistent()
        .get(&DataKey::Frozen(account_id.clone()))
        .unwrap_or(false);
    if !is_frozen {
        panic!("NotFrozen: account is not frozen");
    }
    env.storage()
        .persistent()
        .remove(&DataKey::Frozen(account_id.clone()));
}

pub fn is_frozen(env: &Env, account_id: &Address) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::Frozen(account_id.clone()))
        .unwrap_or(false)
}
