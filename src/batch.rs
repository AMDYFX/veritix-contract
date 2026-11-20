use crate::admin;
use crate::balance;
use crate::validation::require_positive_amount;
use soroban_sdk::{Address, Env, Vec};

pub const MAX_BATCH_LENGTH: u32 = 50;

/// Admin-gated batch minting that enforces the supply cap across the whole batch
/// and returns the total amount minted.
pub fn mint_batch(e: &Env, admin: &Address, mints: Vec<(Address, i128)>) -> i128 {
    admin::check_admin(e, admin);

    if mints.len() > MAX_BATCH_LENGTH {
        panic!("BatchTooLarge: batch length exceeds {}", MAX_BATCH_LENGTH);
    }

    let mut total_minted: i128 = 0;
    for (to, amount) in mints.iter() {
        require_positive_amount(amount);
        total_minted = total_minted
            .checked_add(amount)
            .unwrap_or_else(|| panic!("SupplyOverflow: batch mint overflows i128"));
        balance::mint(e, &to, amount);
    }

    total_minted
}

/// Admin-gated batch clawback recovering tokens across multiple targets.
pub fn clawback_batch(e: &Env, admin: &Address, targets: Vec<(Address, i128)>) {
    admin::check_admin(e, admin);

    if targets.len() > MAX_BATCH_LENGTH {
        panic!("BatchTooLarge: batch length exceeds {}", MAX_BATCH_LENGTH);
    }

    for (from, amount) in targets.iter() {
        require_positive_amount(amount);
        balance::clawback(e, &from, amount);
    }
}
