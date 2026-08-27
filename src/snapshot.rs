use crate::balance::balance_of;
use crate::storage_types::DataKey;
use soroban_sdk::{Address, Env};

pub const SNAPSHOT_LIFETIME_THRESHOLD: u32 = 6_310_000;

pub fn take_snapshot(e: &Env, admin: &Address, account: &Address) {
    crate::admin::check_admin(e, admin);
    let bal = balance_of(e, account);
    e.storage()
        .persistent()
        .set(&DataKey::Snapshot(account.clone()), &bal);
    e.storage().persistent().set(
        &DataKey::SnapshotAt(account.clone()),
        &e.ledger().sequence(),
    );
    e.events().publish(
        (soroban_sdk::symbol_short!("snapshot"),),
        (admin.clone(), account.clone(), bal),
    );
}

pub fn get_snapshot_balance(e: &Env, account: &Address) -> i128 {
    e.storage()
        .persistent()
        .get(&DataKey::Snapshot(account.clone()))
        .unwrap_or(0)
}

pub fn snapshot_taken_at(e: &Env, account: &Address) -> u32 {
    e.storage()
        .persistent()
        .get(&DataKey::SnapshotAt(account.clone()))
        .unwrap_or(0)
}

pub fn is_snapshot_available(e: &Env, account: &Address) -> bool {
    e.storage()
        .persistent()
        .has(&DataKey::Snapshot(account.clone()))
}
