//! Mutations for payment splits.

use crate::storage_types::{DataKey, SplitRecord};
use soroban_sdk::{Address, Env};

/// Replaces one recipient without changing that recipient's share.
///
/// # Panics
///
/// Panics when the split does not exist, `caller` is not its sender, the split
/// has already been distributed, or `old` is not a recipient.
/// Replaces `old_recipient` with `new_recipient` on `split_id`, panicking if
    /// `new_recipient` is already a recipient in the split.
    pub fn replace_split_recipient(
        env: &Env,
        split_id: u64,
        old_recipient: Address,
        new_recipient: Address,
    ) -> Result<(), SplitterError> {
        let mut record: SplitRecord = env
            .storage()
            .persistent()
            .get(&SplitterDataKey::Split(split_id))
            .ok_or(SplitterError::SplitNotFound)?;

        record.sender.require_auth();

        // Prevent duplicate recipients that would cause double payouts
        for entry in record.recipients.iter() {
            if entry.recipient == new_recipient {
                panic!("duplicate recipient in split");
            }
        }

        let mut updated = Vec::new(env);
        let mut found = false;
        for mut item in record.recipients.iter() {
            if item.recipient == old_recipient {
                item.recipient = new_recipient.clone();
                found = true;
            }
            updated.push_back(item);
        }

        if !found {
            return Err(SplitterError::RecipientNotFound);
        }

        record.recipients = updated;
        env.storage()
            .persistent()
            .set(&SplitterDataKey::Split(split_id), &record);
        Ok(())
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

use soroban_sdk::{contracterror, contracttype, Address, Env, Vec};

pub const MAX_SPLITS_PER_SENDER: u32 = 256;
pub const MAX_PAGE_SIZE: u32 = 50;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SplitterError {
    NotAuthorized = 1,
    SplitNotFound = 2,
    RecipientNotFound = 3,
    SenderIndexLimitReached = 4,
    InvalidSplitConfig = 5,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitRecipient {
    pub recipient: Address,
    pub bps: u32, // Basis points (e.g. 10_000 = 100%)
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitRecord {
    pub id: u64,
    pub sender: Address,
    pub recipients: Vec<SplitRecipient>,
    pub active: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SplitterDataKey {
    SplitCounter,
    Split(u64),
    SenderSplits(Address),
}
if recipients.is_empty() {
            return Err(SplitterError::InvalidSplitConfig);
        }

        let mut total_bps: u32 = 0;
        for entry in recipients.iter() {
            if entry.bps == 0 {
                return Err(SplitterError::InvalidSplitConfig);
            }
            total_bps = total_bps.saturating_add(entry.bps);
        }

        if total_bps != TOTAL_BPS {
            // Includes the actual sum so callers/logs immediately see what was provided
            panic!("split shares must total 10000 bps, got {}", total_bps);
        }

        pub const TOTAL_BPS: u32 = 10_000;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitShare {
    pub recipient: Address,
    pub bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitRecord {
    pub id: u64,
    pub sender: Address,
    pub token: Address,
    pub total_amount: i128,
    pub shares: Vec<SplitShare>,
}
pub fn create_split(
        env: &Env,
        sender: Address,
        token: Address,
        total_amount: i128,
        recipients: Vec<Address>,
        shares_bps: Vec<u32>,
    ) -> Result<u64, SplitterError> {
        sender.require_auth();

        if recipients.is_empty()
            || shares_bps.is_empty()
            || recipients.len() != shares_bps.len()
        {
            return Err(SplitterError::InvalidSplitConfig);
        }

        if total_amount <= 0 {
            return Err(SplitterError::InvalidAmount);
        }

        let mut total_bps: u32 = 0;
        let mut shares: Vec<SplitShare> = Vec::new(env);

        for i in 0..recipients.len() {
            let recipient = recipients.get(i).unwrap();
            let bps = shares_bps.get(i).unwrap();

            if bps == 0 {
                return Err(SplitterError::InvalidSplitConfig);
            }

            total_bps = total_bps.saturating_add(bps);
            shares.push_back(SplitShare { recipient, bps });
        }

        if total_bps != TOTAL_BPS {
            panic!("split shares must total 10000 bps, got {}", total_bps);
        }

        let next_id: u64 = env
            .storage()
            .persistent()
            .get(&SplitterDataKey::NextSplitId)
            .unwrap_or(1)
            + 1;
        let split_id = next_id - 1;

        let record = SplitRecord {
            id: split_id,
            sender,
            token,
            total_amount,
            shares,
        };

        env.storage()
            .persistent()
            .set(&SplitterDataKey::NextSplitId, &next_id);
        env.storage()
            .persistent()
            .set(&SplitterDataKey::Split(split_id), &record);

        Ok(split_id)
    }
    