use crate::admin;
use crate::balance;
use crate::storage_types::DataKey;
use crate::validation::require_positive_amount;
use soroban_sdk::{Address, Env, Vec};

/// Takes a snapshot of balances for a list of addresses under `snapshot_id`.
pub fn take_snapshot(e: &Env, admin: &Address, snapshot_id: u32, holders: Vec<Address>) {
    admin::check_admin(e, admin);

    let mut total: i128 = 0;
    for holder in holders.iter() {
        let bal = balance::balance_of(e, &holder);
        e.storage().persistent().set(
            &DataKey::Snapshot(snapshot_id as u64, holder.clone()),
            &bal,
        );
        total = total
            .checked_add(bal)
            .unwrap_or_else(|| panic!("SnapshotOverflow: balances overflow i128"));
    }

    e.storage()
        .persistent()
        .set(&DataKey::SnapshotTotal(snapshot_id as u64), &total);
}

/// Reads the snapshot balance for an address under `snapshot_id`.
pub fn get_snapshot(e: &Env, snapshot_id: u32, holder: &Address) -> Option<i128> {
    e.storage()
        .persistent()
        .get(&DataKey::Snapshot(snapshot_id as u64, holder.clone()))
}

/// Distributes a dividend of `total` among `holders` pro rata to their snapshot balance.
///
/// Any holder without a snapshot balance is rejected.
/// The remainder (rounding dust) is credited to the final holder.
pub fn distribute_dividend(
    e: &Env,
    admin: &Address,
    snapshot_id: u32,
    total: i128,
    holders: Vec<Address>,
) {
    admin::check_admin(e, admin);
    require_positive_amount(total);

    if holders.is_empty() {
        panic!("EmptyHolders: at least one holder required");
    }

    let snapshot_total: i128 = e
        .storage()
        .persistent()
        .get(&DataKey::SnapshotTotal(snapshot_id as u64))
        .unwrap_or(0);

    if snapshot_total == 0 {
        panic!("NoSnapshotTotal: snapshot total is zero or missing");
    }

    let mut remaining = total;
    let len = holders.len();

    for (i, holder) in holders.iter().enumerate() {
        let snap_bal = get_snapshot(e, snapshot_id, &holder)
            .unwrap_or_else(|| panic!("MissingSnapshot: holder has no snapshot"));

        let share = if i == (len as usize - 1) {
            remaining
        } else {
            let s = total
                .checked_mul(snap_bal)
                .unwrap_or_else(|| panic!("DividendOverflow"))
                / snapshot_total;
            remaining = remaining
                .checked_sub(s)
                .unwrap_or_else(|| panic!("DividendUnderflow"));
            s
        };

        if share > 0 {
            balance::transfer(e, admin, &holder, share);
        }
    }
}