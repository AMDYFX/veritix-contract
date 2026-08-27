#![cfg(test)]

use crate::contract::{VeriTixPay, VeriTixPayClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

struct TestEnv<'a> {
    e: Env,
    client: VeriTixPayClient<'a>,
    admin: Address,
}

fn setup() -> TestEnv<'static> {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    TestEnv { e, client, admin }
}

#[test]
fn test_take_snapshot_records_balance() {
    let t = setup();
    let user = Address::generate(&t.e);
    let seq = t.e.ledger().sequence();
    t.client.mint(&t.admin, &user, &1000);

    t.client.take_snapshot(&t.admin, &user);
    assert_eq!(t.client.get_snapshot_balance(&user), 1000);
    assert_eq!(t.client.snapshot_taken_at(&user), seq);
}

#[test]
fn test_snapshot_balance_is_frozen_at_capture() {
    let t = setup();
    let user = Address::generate(&t.e);
    t.client.mint(&t.admin, &user, &1000);
    t.client.take_snapshot(&t.admin, &user);

    // Balance changes after snapshot do not affect the recorded snapshot.
    t.client.mint(&t.admin, &user, &500);
    assert_eq!(t.client.balance(&user), 1500);
    assert_eq!(t.client.get_snapshot_balance(&user), 1000);
}

#[test]
fn test_snapshot_balance_defaults_zero_for_unknown_account() {
    let t = setup();
    let stranger = Address::generate(&t.e);
    assert_eq!(t.client.get_snapshot_balance(&stranger), 0);
    assert_eq!(t.client.snapshot_taken_at(&stranger), 0);
}

#[test]
fn test_snapshot_can_be_overwritten() {
    let t = setup();
    let user = Address::generate(&t.e);
    t.client.mint(&t.admin, &user, &1000);
    t.client.take_snapshot(&t.admin, &user);
    assert_eq!(t.client.get_snapshot_balance(&user), 1000);

    t.client.mint(&t.admin, &user, &3000);
    t.client.take_snapshot(&t.admin, &user);
    assert_eq!(t.client.get_snapshot_balance(&user), 4000);
}

#[test]
fn test_snapshot_zero_balance_account() {
    let t = setup();
    let user = Address::generate(&t.e);
    let seq = t.e.ledger().sequence();
    t.client.take_snapshot(&t.admin, &user);
    assert_eq!(t.client.get_snapshot_balance(&user), 0);
    assert_eq!(t.client.snapshot_taken_at(&user), seq);
}
