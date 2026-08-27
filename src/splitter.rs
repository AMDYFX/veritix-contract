use crate::storage_types::DataKey;
use soroban_sdk::{contracttype, Address, Env, Vec};

#[contracttype]
#[derive(Clone)]
pub struct SplitRecord {
    pub id: u32,
    pub sender: Address,
    pub recipients: Vec<(Address, u32)>, // (address, share_bps)
    pub token: Address,
    pub total_amount: i128,
    pub event_ledger: u32,
    pub distributed: bool,
    pub cancelled: bool,
}

pub fn load_record(e: &Env, split_id: u32) -> SplitRecord {
    e.storage()
        .persistent()
        .get(&DataKey::Split(split_id))
        .expect("split not found")
}

pub(crate) fn save_record(e: &Env, record: &SplitRecord) {
    e.storage()
        .persistent()
        .set(&DataKey::Split(record.id), record);
}

pub fn create_split(
    e: Env,
    sender: Address,
    recipients: Vec<(Address, u32)>,
    token: Address,
    total_amount: i128,
    event_ledger: u32,
) -> u32 {
    sender.require_auth();

    assert!(!recipients.is_empty(), "must have at least one recipient");
    assert!(
        event_ledger > e.ledger().sequence(),
        "event_ledger must be in the future"
    );

    // Validate all share_bps sum to 10000
    let mut total_bps: u32 = 0;
    for i in 0..recipients.len() {
        let (_, bps) = recipients.get(i).unwrap();
        total_bps += bps;
    }
    assert!(total_bps == 10000, "total basis points must equal 10000");

    assert!(total_amount > 0, "total amount must be greater than zero");

    // Pull tokens from sender into the contract
    let token_client = soroban_sdk::token::Client::new(&e, &token);
    token_client.transfer(&sender, &e.current_contract_address(), &total_amount);

    let id: u32 = e
        .storage()
        .persistent()
        .get(&DataKey::SplitCount)
        .unwrap_or(0);

    let record = SplitRecord {
        id,
        sender,
        recipients,
        token,
        total_amount,
        event_ledger,
        distributed: false,
        cancelled: false,
    };

    save_record(&e, &record);
    e.storage()
        .persistent()
        .set(&DataKey::SplitCount, &(id + 1));

    // Emit split created event
    e.events().publish(
        (
            soroban_sdk::symbol_short!("split_cr"),
            record.sender.clone(),
        ),
        (record.id, record.total_amount),
    );

    id
}

pub fn distribute_split(e: Env, caller: Address, split_id: u32) {
    caller.require_auth();
    crate::pause::require_not_paused(&e);

    let mut record = load_record(&e, split_id);

    assert!(!record.distributed, "already distributed");
    assert!(!record.cancelled, "split has been cancelled");
    assert!(
        caller == record.sender || crate::admin::is_admin(&e, &caller),
        "not authorised to distribute"
    );

    record.distributed = true;
    save_record(&e, &record);

    // Pay each recipient their share, awarding any remainder (dust) to the
    // first recipient so no stroop is ever lost.
    let token_client = soroban_sdk::token::Client::new(&e, &record.token);
    let mut allocated: i128 = 0;
    for i in 0..record.recipients.len() {
        let (recipient, bps) = record.recipients.get(i).unwrap();
        let amount = record.total_amount * bps as i128 / 10000;
        token_client.transfer(&e.current_contract_address(), recipient, &amount);
        allocated += amount;
    }
    // Hand the rounding remainder to the first recipient if any stroops are left.
    if allocated < record.total_amount {
        let (first, _) = record.recipients.get(0).unwrap();
        token_client.transfer(
            &e.current_contract_address(),
            first,
            &(record.total_amount - allocated),
        );
    }
}

pub fn cancel_split(e: Env, caller: Address, split_id: u32) {
    caller.require_auth();

    let mut record = load_record(&e, split_id);

    assert!(!record.distributed, "already distributed");
    assert!(!record.cancelled, "already cancelled");
    assert!(
        caller == record.sender || crate::admin::is_admin(&e, &caller),
        "not authorised to cancel"
    );

    record.cancelled = true;
    save_record(&e, &record);

    // Return all funds to the sender
    let token_client = soroban_sdk::token::Client::new(&e, &record.token);
    token_client.transfer(
        &e.current_contract_address(),
        &record.sender,
        &record.total_amount,
    );
}

pub fn replace_split_recipient(
    e: Env,
    sender: Address,
    split_id: u32,
    old_recipient: Address,
    new_recipient: Address,
) {
    sender.require_auth();

    let mut record = load_record(&e, split_id);

    // Verify sender is the split creator
    assert!(
        sender == record.sender,
        "not authorised to replace recipient"
    );
    // Verify split is not distributed or cancelled
    assert!(!record.distributed, "split has already been distributed");
    assert!(!record.cancelled, "split has been cancelled");

    // Find old_recipient in recipients list
    let mut found_index: Option<usize> = None;
    let mut old_bps: u32 = 0;
    for i in 0..record.recipients.len() {
        let (addr, bps) = record.recipients.get(i).unwrap();
        if addr == old_recipient {
            found_index = Some(i as usize);
            old_bps = bps;
            break;
        }
    }
    assert!(found_index.is_some(), "old recipient not found in split");

    // Verify new_recipient is not already in the list (no duplicates)
    for i in 0..record.recipients.len() {
        let (addr, _) = record.recipients.get(i).unwrap();
        if addr == new_recipient {
            panic!("new recipient is already in the split");
        }
    }

    // Replace in place, preserving share_bps
    let index = found_index.unwrap();
    record
        .recipients
        .set(index as u32, (new_recipient.clone(), old_bps));

    // Save the updated record
    save_record(&e, &record);

    // Emit the split_repl event
    e.events().publish(
        (
            soroban_sdk::symbol_short!("splt_rpl"),
            split_id,
            old_recipient,
            new_recipient,
        ),
        (),
    );
}

use soroban_sdk::{Address, Env, Symbol};
use crate::storage_types::DataKey;

pub const MAX_SPLIT_FEE_BPS: u32 = 200; // Maximum 2% protocol fee

pub fn set_split_fee_config(e: &Env, admin: &Address, fee_bps: u32, treasury: &Address) {
    admin.require_auth();
    
    if fee_bps > MAX_SPLIT_FEE_BPS {
        panic!("Split protocol fee exceeds maximum allowed basis points (200)");
    }

    e.storage().instance().set(&DataKey::SplitProtocolFeeBps, &fee_bps);
    e.storage().instance().set(&DataKey::SplitProtocolTreasury, treasury);
}

pub fn calculate_split_fee(e: &Env, total_amount: i128) -> (i128, Option<(Address, i128)>) {
    let fee_bps: u32 = e.storage().instance().get(&DataKey::SplitProtocolFeeBps).unwrap_or(0);
    
    if fee_bps == 0 {
        return (total_amount, None);
    }

    let fee_amount = (total_amount * fee_bps as i128) / 10000;
    let remainder_amount = total_amount - fee_amount;

    if let Some(treasury) = e.storage().instance().get::<DataKey, Address>(&DataKey::SplitProtocolTreasury) {
        (remainder_amount, Some((treasury, fee_amount)))
    } else {
        (total_amount, None)
    }
}