use crate::storage_types::DataKey;
use soroban_sdk::{contracttype, Address, Env, Vec};

#[contracttype]
#[derive(Clone)]
pub struct MultiEscrowRecord {
    pub id: u32,
    pub depositor: Address,
    pub recipients: Vec<(Address, i128)>, // (address, share_amount)
    pub token: Address,
    pub total_amount: i128,
    pub expiry_ledger: u32,
    pub released: bool,
    pub refunded: bool,
}

fn get_admin(e: &Env) -> Address {
    e.storage()
        .persistent()
        .get(&DataKey::Admin)
        .expect("admin not set")
}

pub fn create_multi_escrow(
    e: Env,
    depositor: Address,
    recipients: Vec<(Address, i128)>,
    token: Address,
    expiry_ledger: u32,
) -> u32 {
    depositor.require_auth();

    assert!(!recipients.is_empty(), "must have at least one recipient");
    assert!(
        expiry_ledger > e.ledger().sequence(),
        "expiry_ledger must be in the future"
    );

    // Compute the total and validate every share is positive
    let mut total_amount: i128 = 0;
    for i in 0..recipients.len() {
        let (_, share) = recipients.get(i).unwrap();
        assert!(share > 0, "each recipient share must be greater than zero");
        total_amount += share;
    }

    assert!(total_amount > 0, "total amount must be greater than zero");

    // Pull and bump the counter
    let id: u32 = e
        .storage()
        .persistent()
        .get(&DataKey::MultiEscrowCount)
        .unwrap_or(0);

    // Transfer total from depositor into contract
    let token_client = soroban_sdk::token::Client::new(&e, &token);
    token_client.transfer(&depositor, &e.current_contract_address(), &total_amount);

    let record = MultiEscrowRecord {
        id,
        depositor,
        recipients,
        token,
        total_amount,
        expiry_ledger,
        released: false,
        refunded: false,
    };

    e.storage()
        .persistent()
        .set(&DataKey::MultiEscrow(id), &record);
    e.storage()
        .persistent()
        .set(&DataKey::MultiEscrowCount, &(id + 1));

    id
}

pub fn release_multi_escrow(e: Env, caller: Address, multi_escrow_id: u32) {
    caller.require_auth();

    let mut record: MultiEscrowRecord = e
        .storage()
        .persistent()
        .get(&DataKey::MultiEscrow(multi_escrow_id))
        .expect("multi-escrow not found");

    assert!(!record.released, "already released");
    assert!(!record.refunded, "already refunded");
    assert!(
        caller == record.depositor || caller == get_admin(&e),
        "not authorised to release"
    );
    assert!(
        e.ledger().sequence() <= record.expiry_ledger,
        "multi-escrow has expired"
    );

    record.released = true;
    e.storage()
        .persistent()
        .set(&DataKey::MultiEscrow(multi_escrow_id), &record);

    // Pay each recipient their exact share
    let token_client = soroban_sdk::token::Client::new(&e, &record.token);
    for i in 0..record.recipients.len() {
        let (recipient, share) = record.recipients.get(i).unwrap();
        token_client.transfer(&e.current_contract_address(), &recipient, &share);
    }
}

pub fn refund_multi_escrow(e: Env, caller: Address, multi_escrow_id: u32) {
    caller.require_auth();

    let mut record: MultiEscrowRecord = e
        .storage()
        .persistent()
        .get(&DataKey::MultiEscrow(multi_escrow_id))
        .expect("multi-escrow not found");

    assert!(!record.released, "already released");
    assert!(!record.refunded, "already refunded");
    assert!(
        caller == record.depositor || caller == get_admin(&e),
        "not authorised to refund"
    );

    record.refunded = true;
    e.storage()
        .persistent()
        .set(&DataKey::MultiEscrow(multi_escrow_id), &record);

    // Return the entire pooled amount to the depositor
    let token_client = soroban_sdk::token::Client::new(&e, &record.token);
    token_client.transfer(
        &e.current_contract_address(),
        &record.depositor,
        &record.total_amount,
    );
}
