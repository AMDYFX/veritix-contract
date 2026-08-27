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
fn test_pause_defaults_false() {
    let t = setup();
    assert!(!t.client.is_paused());
}

#[test]
fn test_set_paused_toggles_state() {
    let t = setup();
    t.client.set_paused(&t.admin, &true);
    assert!(t.client.is_paused());
    t.client.set_paused(&t.admin, &false);
    assert!(!t.client.is_paused());
}

#[test]
fn test_set_paused_requires_admin() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    client.set_paused(&admin, &true);
    assert!(client.is_paused());
}

#[test]
#[should_panic(expected = "ContractPaused: contract is paused")]
fn test_transfer_from_blocked_when_paused() {
    let t = setup();
    let from = Address::generate(&t.e);
    let to = Address::generate(&t.e);
    let spender = Address::generate(&t.e);
    t.client.mint(&t.admin, &from, &500);
    let expiry = t.e.ledger().sequence() + 100;
    t.client.approve(&from, &spender, &500, &expiry);

    t.client.set_paused(&t.admin, &true);
    t.client.transfer_from(&spender, &from, &to, &100);
}

#[test]
#[should_panic(expected = "ContractPaused: contract is paused")]
fn test_distribute_split_blocked_when_paused() {
    let t = setup();
    let sender = Address::generate(&t.e);
    let recipient = Address::generate(&t.e);
    let token = t.e.register_stellar_asset_contract(sender.clone());
    soroban_sdk::token::StellarAssetClient::new(&t.e, &token).mint(&sender, &1000);

    let recipients = soroban_sdk::vec![&t.e, (recipient.clone(), 10000u32)];
    let event_ledger = t.e.ledger().sequence() + 1000;
    let split_id = t.e.as_contract(&t.client.address, || {
        crate::splitter::create_split(
            t.e.clone(),
            sender.clone(),
            recipients,
            token.clone(),
            1000,
            event_ledger,
        )
    });

    t.client.set_paused(&t.admin, &true);
    t.e.as_contract(&t.client.address, || {
        crate::splitter::distribute_split(t.e.clone(), sender, split_id);
    });
}

#[test]
fn test_unpause_re_allows_transfer() {
    let t = setup();
    let from = Address::generate(&t.e);
    let to = Address::generate(&t.e);
    let spender = Address::generate(&t.e);
    t.client.mint(&t.admin, &from, &500);
    let expiry = t.e.ledger().sequence() + 100;
    t.client.approve(&from, &spender, &500, &expiry);

    t.client.set_paused(&t.admin, &true);
    t.client.set_paused(&t.admin, &false);
    t.client.transfer_from(&spender, &from, &to, &100);
    assert_eq!(t.client.balance(&to), 100);
}
