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
fn test_mint_i128_max_succeeds() {
    let t = setup();
    let user = Address::generate(&t.e);
    t.client.mint(&t.admin, &user, &i128::MAX);
    assert_eq!(t.client.balance(&user), i128::MAX);
    assert_eq!(t.client.total_supply(), i128::MAX);
}

#[test]
#[should_panic(expected = "supply overflow")]
fn test_mint_overflow_second_mint_panics() {
    let t = setup();
    let user = Address::generate(&t.e);
    // First mint fills the supply, second mint overflows.
    t.client.mint(&t.admin, &user, &i128::MAX);
    t.client.mint(&t.admin, &user, &1);
}

#[test]
#[should_panic(expected = "supply overflow")]
fn test_mint_overflow_with_existing_balance_panics() {
    let t = setup();
    let user = Address::generate(&t.e);
    t.client.mint(&t.admin, &user, &1000);
    t.client.mint(&t.admin, &user, &(i128::MAX - 500));
}

#[test]
#[should_panic(expected = "Amount must be strictly positive")]
fn test_burn_zero_panics() {
    let t = setup();
    let user = Address::generate(&t.e);
    t.client.mint(&t.admin, &user, &100);
    t.client.burn(&user, &0);
}

#[test]
#[should_panic(expected = "insufficient balance")]
fn test_burn_amount_exceeds_balance_panics() {
    let t = setup();
    let user = Address::generate(&t.e);
    t.client.mint(&t.admin, &user, &10);
    t.client.burn(&user, &11);
}

#[test]
#[should_panic(expected = "Amount must be strictly positive")]
fn test_approve_zero_panics() {
    let t = setup();
    let from = Address::generate(&t.e);
    let spender = Address::generate(&t.e);
    let expiry = t.e.ledger().sequence() + 100;
    t.client.approve(&from, &spender, &0, &expiry);
}
