//! Bulk whitelist onboarding.

use crate::storage_types::DataKey;
use soroban_sdk::{Address, Env, Vec};

/// Maximum batch size for bulk whitelisting.
pub const MAX_BATCH_SIZE: u32 = 50;

/// Adds a batch of accounts to the whitelist, checking admin authorization once.
///
/// # Panics
///
/// Panics when caller is not admin, or when batch size exceeds [`MAX_BATCH_SIZE`].
pub fn add_to_whitelist_batch(e: &Env, admin: &Address, accounts: Vec<Address>) {
    crate::admin::check_admin(e, admin);
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
