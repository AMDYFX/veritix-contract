use crate::admin;
use crate::storage_types::DataKey;
use soroban_sdk::{Address, Env};

/// Whether whitelist enforcement is enabled. Unset storage defaults to false.
///
/// Contract-wide configuration, so it lives in instance storage with the admin
/// and the supply counters (see `docs/ttl.md`).
pub fn is_whitelist_enabled(e: &Env) -> bool {
    e.storage()
        .instance()
        .get(&DataKey::WhitelistEnabled)
        .unwrap_or(false)
}

/// Enables whitelist enforcement for the contract.
pub fn enable_whitelist(e: &Env, admin_addr: &Address) {
    admin::check_admin(e, admin_addr);
    e.storage()
        .instance()
        .set(&DataKey::WhitelistEnabled, &true);
}

/// Disables whitelist enforcement for the contract.
pub fn disable_whitelist(e: &Env, admin_addr: &Address) {
    admin::check_admin(e, admin_addr);
    e.storage()
        .instance()
        .set(&DataKey::WhitelistEnabled, &false);
}

/// Adds `account` to the whitelist. Admin-only.
pub fn add_to_whitelist(e: &Env, admin_addr: &Address, account: &Address) {
    admin::check_admin(e, admin_addr);
    e.storage()
        .persistent()
        .set(&DataKey::Whitelisted(account.clone()), &true);
}

/// Removes `account` from the whitelist. Admin-only.
pub fn remove_from_whitelist(e: &Env, admin_addr: &Address, account: &Address) {
    admin::check_admin(e, admin_addr);
    e.storage()
        .persistent()
        .remove(&DataKey::Whitelisted(account.clone()));
}

/// Whether `account` is whitelisted.
pub fn is_whitelisted(e: &Env, account: &Address) -> bool {
    e.storage()
        .persistent()
        .get(&DataKey::Whitelisted(account.clone()))
        .unwrap_or(false)
}

/// Panics when whitelist is enabled and `account` is not whitelisted.
pub fn require_whitelisted(e: &Env, account: &Address) {
    if is_whitelist_enabled(e) && !is_whitelisted(e, account) {
        panic!("NotWhitelisted: account is not whitelisted");
    }
}
