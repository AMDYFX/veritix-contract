//! Nonce-protected signed whitelist onboarding.

use crate::storage_types::DataKey;
use crate::whitelist_batch::MAX_BATCH_SIZE;
use soroban_sdk::{Address, Env, Vec};

/// Returns the current nonce for `account`.
pub fn get_nonce(e: &Env, account: &Address) -> u64 {
    e.storage()
        .persistent()
        .get(&DataKey::Counter(account.clone()))
        .unwrap_or(0)
}

/// Adds accounts to the whitelist using a per-admin sequence nonce. Reused nonce panics.
///
/// # Panics
///
/// Panics when caller is not admin, or when `nonce` does not match the stored nonce,
/// or when batch size exceeds [`MAX_BATCH_SIZE`].
pub fn add_to_whitelist_signed(e: &Env, admin: &Address, accounts: Vec<Address>, nonce: u64) {
    crate::admin::check_admin(e, admin);
    let expected_nonce = get_nonce(e, admin);
    if nonce != expected_nonce {
        panic!(
            "InvalidNonce: expected nonce {} but got {}",
            expected_nonce, nonce
        );
    }
    let next_nonce = expected_nonce
        .checked_add(1)
        .unwrap_or_else(|| panic!("NonceOverflow: nonce overflow u64"));
    e.storage()
        .persistent()
        .set(&DataKey::Counter(admin.clone()), &next_nonce);

    if accounts.len() > MAX_BATCH_SIZE {
        panic!(
            "BatchTooLarge: batch size {} exceeds maximum of {}",
            accounts.len(),
            MAX_BATCH_SIZE
        );
    }
    for account in accounts.iter() {
        e.storage()
            .persistent()
            .set(&DataKey::Whitelisted(account), &true);
    }
}
