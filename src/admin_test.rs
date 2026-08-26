#![cfg(test)]

use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Address, Env};
use crate::contract::{VeriTixPay, VeriTixPayClient};

struct TestEnv<'a> {
    e: Env,
    client: VeriTixPayClient<'a>,
    admin: Address,
    new_admin: Address,
}

fn setup() -> TestEnv<'static> {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    let new_admin = Address::generate(&e);

    client.initialize(&admin);

    TestEnv { e, client, admin, new_admin }
}

#[test]
fn test_transfer_ownership_sets_proposed_admin() {
    let t = setup();
    t.client.transfer_ownership(&t.new_admin);
}

#[test]
#[should_panic(expected = "cannot propose current admin")]
fn test_transfer_ownership_self_proposal_panics() {
    let t = setup();
    t.client.transfer_ownership(&t.admin);
}

#[test]
fn test_accept_admin_sets_new_admin_with_delay() {
    let t = setup();
    t.client.transfer_ownership(&t.new_admin);
    t.client.accept_admin(&t.new_admin);

    let active_after = t.client.admin_active_after_ledger();
    assert!(active_after > t.e.ledger().sequence());
}

#[test]
fn test_admin_active_after_ledger_returns_zero_when_not_set() {
    let e = Env::default();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    assert_eq!(client.admin_active_after_ledger(), 0);
}

#[test]
fn test_old_admin_still_active_during_delay() {
    let t = setup();
    t.client.transfer_ownership(&t.new_admin);
    t.client.accept_admin(&t.new_admin);

    t.client.transfer_ownership(&Address::generate(&t.e));
}

#[test]
fn test_full_ownership_transfer_lifecycle() {
    let t = setup();

    t.client.transfer_ownership(&t.new_admin);
    t.client.accept_admin(&t.new_admin);

    let active_after = t.client.admin_active_after_ledger();
    assert!(active_after > t.e.ledger().sequence());

    t.e.ledger().with_mut(|l| l.sequence_number = active_after);
    t.client.transfer_ownership(&Address::generate(&t.e));
}
