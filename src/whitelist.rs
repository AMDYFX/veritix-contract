use crate::storage_types::DataKey;
use soroban_sdk::{symbol_short, xdr::ToXdr, Address, Bytes, BytesN, Env, Vec};

pub fn enable(e: &Env, admin: &Address) {
    crate::admin::check_admin(e, admin);
    e.storage()
        .persistent()
        .set(&DataKey::WhitelistEnabled, &true);
}

pub fn disable(e: &Env, admin: &Address) {
    crate::admin::check_admin(e, admin);
    e.storage().persistent().remove(&DataKey::WhitelistEnabled);
}

pub fn is_enabled(e: &Env) -> bool {
    e.storage()
        .persistent()
        .get(&DataKey::WhitelistEnabled)
        .unwrap_or(false)
}

pub fn add(e: &Env, admin: &Address, account: &Address) {
    crate::admin::check_admin(e, admin);
    e.storage()
        .persistent()
        .set(&DataKey::Whitelisted(account.clone()), &true);
}

pub fn remove(e: &Env, admin: &Address, account: &Address) {
    crate::admin::check_admin(e, admin);
    e.storage()
        .persistent()
        .remove(&DataKey::Whitelisted(account.clone()));
}

pub fn is_whitelisted(e: &Env, account: &Address) -> bool {
    if !is_enabled(e) {
        return true;
    }
    e.storage()
        .persistent()
        .get(&DataKey::Whitelisted(account.clone()))
        .unwrap_or(false)
}

/// #741: batch add — whitelists up to 50 accounts in a single admin call.
pub fn add_to_whitelist_batch(e: &Env, admin: &Address, accounts: &Vec<Address>) {
    crate::admin::check_admin(e, admin);
    if accounts.len() > 50 {
        panic!("TooManyAccounts: maximum 50 accounts per batch");
    }
    for i in 0..accounts.len() {
        e.storage()
            .persistent()
            .set(&DataKey::Whitelisted(accounts.get(i).unwrap().clone()), &true);
    }
}

pub fn check(e: &Env, from: &Address, to: &Address) {
    if is_enabled(e) {
        assert!(is_whitelisted(e, from), "sender not whitelisted");
        assert!(is_whitelisted(e, to), "recipient not whitelisted");
    }
}

/// #748: bulk whitelist via a single signed message — the admin signs
/// (symbol || admin || addresses... || nonce) and the contract verifies the
/// signature, increments the admin nonce to prevent replay, and whitelists
/// every address in one call. Max 200 addresses per call.
pub fn add_to_whitelist_signed(
    e: &Env,
    admin: &Address,
    addresses: &Vec<Address>,
    nonce: u64,
    public_key: &BytesN<32>,
    signature: &BytesN<64>,
) {
    if addresses.is_empty() {
        panic!("addresses cannot be empty");
    }
    if addresses.len() > 200 {
        panic!("TooManyAddresses: maximum 200 addresses per call");
    }

    let current_nonce: u64 = e
        .storage()
        .persistent()
        .get(&DataKey::AdminNonce(admin.clone()))
        .unwrap_or(0);
    if nonce != current_nonce {
        panic!("InvalidNonce: expected {} but got {}", current_nonce, nonce);
    }

    let mut msg = Bytes::new(e);
    msg.append(&symbol_short!("wl_sgn").to_xdr(e));
    msg.append(&admin.clone().to_xdr(e));
    for i in 0..addresses.len() {
        msg.append(&addresses.get(i).unwrap().to_xdr(e));
    }
    msg.append(&nonce.to_xdr(e));
    let hash: soroban_sdk::crypto::Hash<32> = e.crypto().sha256(&msg);
    let hash_bytes: Bytes = hash.into();
    e.crypto().ed25519_verify(public_key, &hash_bytes, signature);

    e.storage()
        .persistent()
        .set(&DataKey::AdminNonce(admin.clone()), &(current_nonce + 1));

    for i in 0..addresses.len() {
        e.storage()
            .persistent()
            .set(&DataKey::Whitelisted(addresses.get(i).unwrap().clone()), &true);
    }

    e.events().publish(
        (symbol_short!("wl_sgn"), admin.clone()),
        addresses.len() as u32,
    );
}
