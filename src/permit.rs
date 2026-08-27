use crate::allowance::write_allowance;
use crate::storage_types::DataKey;
use soroban_sdk::{symbol_short, xdr::ToXdr, Address, Bytes, BytesN, Env, Vec};

fn read_nonce(e: &Env, owner: &Address) -> u64 {
    let key = DataKey::Nonce(owner.clone());
    e.storage().persistent().get(&key).unwrap_or(0)
}

fn write_nonce(e: &Env, owner: &Address, nonce: u64) {
    let key = DataKey::Nonce(owner.clone());
    e.storage().persistent().set(&key, &nonce);
}

pub fn nonces(e: &Env, owner: Address) -> u64 {
    read_nonce(e, &owner)
}

pub fn check_and_increment_nonce(e: &Env, user: &Address, expected_nonce: u32) {
    let current: u64 = read_nonce(e, user);
    if expected_nonce as u64 != current {
        panic!(
            "InvalidNonce: expected {} but got {}",
            current, expected_nonce
        );
    }
    write_nonce(e, user, current + 1);
}

/// #574: Batch permit — sets allowances for multiple spenders via a single signed message.
/// The signed message includes the full `approvals` vector, the owner address, and the nonce.
/// The nonce is incremented exactly once for the entire batch.
pub fn permit_batch(
    e: &Env,
    owner: Address,
    approvals: Vec<(Address, i128, u32)>,
    nonce: u64,
    public_key: BytesN<32>,
    signature: BytesN<64>,
) {
    if approvals.is_empty() {
        panic!("approvals cannot be empty");
    }
    if approvals.len() > 20 {
        panic!("TooManyApprovals: maximum 20 approvals per batch");
    }
    assert!(
        public_key.to_array().len() == 32,
        "invalid public_key byte length"
    );

    let current_nonce = read_nonce(e, &owner);
    if nonce != current_nonce {
        panic!("invalid nonce");
    }

    let hash = hash_permit_batch(e, &owner, &approvals, nonce);
    let hash_bytes: Bytes = hash.into();
    e.crypto()
        .ed25519_verify(&public_key, &hash_bytes, &signature);

    write_nonce(e, &owner, current_nonce + 1);

    for i in 0..approvals.len() {
        let (spender, amount, expiration_ledger) = approvals.get(i).unwrap();
        write_allowance(e, &owner, &spender, amount, expiration_ledger);
    }

    e.events().publish(
        (symbol_short!("permit_bt"), owner.clone()),
        approvals.len() as u32,
    );
}

fn hash_permit_batch(
    e: &Env,
    owner: &Address,
    approvals: &Vec<(Address, i128, u32)>,
    nonce: u64,
) -> BytesN<32> {
    let mut msg = Bytes::new(e);
    msg.append(&symbol_short!("permit_bt").to_xdr(e));
    msg.append(&owner.clone().to_xdr(e));
    for i in 0..approvals.len() {
        let (spender, amount, expiration_ledger) = approvals.get(i).unwrap();
        msg.append(&spender.to_xdr(e));
        msg.append(&amount.to_xdr(e));
        msg.append(&expiration_ledger.to_xdr(e));
    }
    msg.append(&nonce.to_xdr(e));
    let hash: soroban_sdk::crypto::Hash<32> = e.crypto().sha256(&msg);
    hash.into()
}
