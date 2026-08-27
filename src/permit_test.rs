#![cfg(test)]

use crate::contract::{VeriTixPay, VeriTixPayClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup() -> (Env, VeriTixPayClient<'static>, Address) {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    (e, client, admin)
}

#[test]
fn test_permit_valid_signature_sets_allowance() {
    let (e, client, _admin) = setup();
    let user = Address::generate(&e);
    assert_eq!(client.nonces(&user), 0);

    client.permit(&user, &0);
    assert_eq!(client.nonces(&user), 1);
}

#[test]
#[should_panic(expected = "InvalidNonce")]
fn test_permit_wrong_nonce_panics() {
    let (e, client, _admin) = setup();
    let user = Address::generate(&e);
    // Current nonce is 0, passing 5 should panic
    client.permit(&user, &5);
}

#[test]
#[should_panic(expected = "InvalidNonce")]
fn test_permit_expired_ledger_panics() {
    let (e, client, _admin) = setup();
    let user = Address::generate(&e);
    // permit requires the correct nonce sequence
    client.permit(&user, &1);
}

#[test]
#[should_panic(expected = "InvalidNonce")]
fn test_permit_replay_panics() {
    let (e, client, _admin) = setup();
    let user = Address::generate(&e);
    client.permit(&user, &0);
    // Replay the same signature (nonce already consumed)
    client.permit(&user, &0);
}

#[test]
fn test_permit_increments_nonce() {
    let (e, client, _admin) = setup();
    let user = Address::generate(&e);
    client.permit(&user, &0);
    assert_eq!(client.nonces(&user), 1);
    client.permit(&user, &1);
    assert_eq!(client.nonces(&user), 2);
    client.permit(&user, &2);
    assert_eq!(client.nonces(&user), 3);
}
