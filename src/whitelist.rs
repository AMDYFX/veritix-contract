use crate::storage_types::DataKey;
use soroban_sdk::{Address, Env};

pub fn enable(e: &Env, admin: &Address) {
    crate::admin::check_admin(e, admin);
    e.storage()
        .persistent()
        .set(&DataKey::WhitelistEnabled, &true);
}

pub fn disable(e: &Env, admin: &Address) {
    crate::admin::check_admin(e, admin);
    e.storage().persistent().remove(&DataKey::WhitelistEnabled);
}

pub fn is_enabled(e: &Env) -> bool {
    e.storage()
        .persistent()
        .get(&DataKey::WhitelistEnabled)
        .unwrap_or(false)
}

pub fn add(e: &Env, admin: &Address, account: &Address) {
    crate::admin::check_admin(e, admin);
    e.storage()
        .persistent()
        .set(&DataKey::Whitelisted(account.clone()), &true);
}

pub fn remove(e: &Env, admin: &Address, account: &Address) {
    crate::admin::check_admin(e, admin);
    e.storage()
        .persistent()
        .remove(&DataKey::Whitelisted(account.clone()));
}

pub fn is_whitelisted(e: &Env, account: &Address) -> bool {
    if !is_enabled(e) {
        return true;
    }
    e.storage()
        .persistent()
        .get(&DataKey::Whitelisted(account.clone()))
        .unwrap_or(false)
}

pub fn check(e: &Env, from: &Address, to: &Address) {
    if is_enabled(e) {
        assert!(is_whitelisted(e, from), "sender not whitelisted");
        assert!(is_whitelisted(e, to), "recipient not whitelisted");
    }
}
