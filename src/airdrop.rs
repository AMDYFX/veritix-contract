use crate::admin;
use crate::balance;
use crate::validation::require_positive_amount;
use soroban_sdk::{token, Address, Env, Vec};

pub const MAX_AIRDROP_HOLDERS: u32 = 100;

/// Admin-gated airdrop paying equal shares across the holder set.
/// Remainder (dust) stays with the admin (or in contract if funded by transfer).
/// If `token` is the current contract address, tokens are transferred from `admin` to holders using internal balance.
/// Otherwise, external token transfer is used via token client.
pub fn airdrop(e: &Env, admin: &Address, token: &Address, total_amount: i128) {
    admin::check_admin(e, admin);
    require_positive_amount(total_amount);

    let holders: Vec<Address> = balance::holders(e);

    let count = holders.len();
    if count == 0 {
        panic!("NoHolders: holder set is empty");
    }
    if count > MAX_AIRDROP_HOLDERS {
        panic!("TooManyHolders: exceeds limit of {}", MAX_AIRDROP_HOLDERS);
    }

    let share = total_amount / (count as i128);
    if share == 0 {
        panic!("ShareTooSmall: total amount insufficient for distribution");
    }

    let is_native = token == &e.current_contract_address();

    for holder in holders.iter() {
        if is_native {
            balance::transfer(e, admin, &holder, share);
        } else {
            token::TokenClient::new(e, token).transfer(admin, &holder, &share);
        }
    }
}
