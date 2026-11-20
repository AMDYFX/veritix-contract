//! Mutations for payment splits.

use crate::storage_types::{DataKey, SplitRecord};
use soroban_sdk::{Address, Env};

/// Replaces one recipient without changing that recipient's share.
///
/// # Panics
///
/// Panics when the split does not exist, `caller` is not its sender, the split
/// has already been distributed, or `old` is not a recipient.
pub fn replace_split_recipient(
    e: &Env,
    caller: &Address,
    split_id: u64,
    old: &Address,
    new: &Address,
) {
    let key = DataKey::Split(split_id);
    let mut split: SplitRecord = e
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| panic!("SplitNotFound: no split with id {}", split_id));

    if &split.sender != caller {
        panic!("Unauthorized: only the split sender may replace recipients");
    }
    caller.require_auth();

    if split.distributed {
        panic!(
            "SplitAlreadyDistributed: split {} has already been distributed",
            split_id
        );
    }

    let recipient_index = split
        .recipients
        .iter()
        .position(|recipient| &recipient == old)
        .and_then(|index| u32::try_from(index).ok())
        .unwrap_or_else(|| {
            panic!(
                "SplitRecipientNotFound: recipient is not in split {}",
                split_id
            )
        });

    split.recipients.set(recipient_index, new.clone());
    e.storage().persistent().set(&key, &split);
}

/// Creates one escrow per recipient for its share of the split and returns the
/// new escrow ids. The last recipient receives any rounding remainder.
///
/// # Panics
///
/// Panics when the split does not exist, `sender` is not its sender, or it has
/// already been distributed.
pub fn split_to_escrow(
    e: &Env,
    sender: &Address,
    split_id: u64,
    deadline_ledger: u32,
) -> soroban_sdk::Vec<u32> {
    let key = DataKey::Split(split_id);
    let mut split: SplitRecord = e
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| panic!("SplitNotFound: no split with id {}", split_id));

    if &split.sender != sender {
        panic!("Unauthorized: only the split sender may convert it to escrows");
    }
    sender.require_auth();
    if split.distributed {
        panic!(
            "SplitAlreadyDistributed: split {} has already been distributed",
            split_id
        );
    }

    split.distributed = true;
    e.storage().persistent().set(&key, &split);

    let mut ids = soroban_sdk::Vec::new(e);
    let last = split.recipients.len().saturating_sub(1);
    let mut allocated: i128 = 0;
    for i in 0..split.recipients.len() {
        let recipient = split.recipients.get(i).unwrap();
        let amount = if i == last {
            split.total_amount - allocated
        } else {
            let share = split.shares_bps.get(i).unwrap() as i128;
            split
                .total_amount
                .checked_mul(share)
                .unwrap_or_else(|| panic!("SplitOverflow: share calculation overflows"))
                / 10_000
        };
        allocated += amount;
        if amount > 0 {
            ids.push_back(crate::escrow::create(
                e,
                sender,
                &recipient,
                &split.token,
                amount,
                deadline_ledger,
            ));
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{vec, Address};

    #[test]
    fn replaces_recipient_without_changing_share() {
        let e = Env::default();
        e.mock_all_auths();
        let sender = Address::generate(&e);
        let old = Address::generate(&e);
        let other = Address::generate(&e);
        let new = Address::generate(&e);
        let split = SplitRecord {
            sender: sender.clone(),
            token: Address::generate(&e),
            total_amount: 1_000,
            recipients: vec![&e, old.clone(), other],
            shares_bps: vec![&e, 6_000, 4_000],
            distributed: false,
        };
        e.storage().persistent().set(&DataKey::Split(7), &split);

        replace_split_recipient(&e, &sender, 7, &old, &new);

        let updated: SplitRecord = e.storage().persistent().get(&DataKey::Split(7)).unwrap();
        assert_eq!(updated.recipients.get(0), Some(new));
        assert_eq!(updated.shares_bps, split.shares_bps);
    }

    #[test]
    #[should_panic(expected = "SplitAlreadyDistributed")]
    fn refuses_replacement_after_distribution() {
        let e = Env::default();
        e.mock_all_auths();
        let sender = Address::generate(&e);
        let old = Address::generate(&e);
        let split = SplitRecord {
            sender: sender.clone(),
            token: Address::generate(&e),
            total_amount: 1_000,
            recipients: vec![&e, old.clone()],
            shares_bps: vec![&e, 10_000],
            distributed: true,
        };
        e.storage().persistent().set(&DataKey::Split(8), &split);

        replace_split_recipient(&e, &sender, 8, &old, &Address::generate(&e));
    }
}
