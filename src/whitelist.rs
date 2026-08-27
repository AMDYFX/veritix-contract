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

/// #741: batch add — whitelists up to 50 accounts in a single admin call.
pub fn add_to_whitelist_batch(e: &Env, admin: &Address, accounts: &Vec<Address>) {
    crate::admin::check_admin(e, admin);
    if accounts.len() > 50 {
        panic!("TooManyAccounts: maximum 50 accounts per batch");
    }
    for i in 0..accounts.len() {
        e.storage()
            .persistent()
            .set(&DataKey::Whitelisted(accounts.get(i).unwrap().clone()), &true);
    }
}

pub fn check(e: &Env, from: &Address, to: &Address) {
    if is_enabled(e) {
        assert!(is_whitelisted(e, from), "sender not whitelisted");
        assert!(is_whitelisted(e, to), "recipient not whitelisted");
    }
}
