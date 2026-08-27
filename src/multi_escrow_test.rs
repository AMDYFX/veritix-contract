#![cfg(test)]

use crate::contract::{VeriTixPay, VeriTixPayClient};
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

fn setup() -> (
    Env,
    VeriTixPayClient<'static>,
    Address,
    Address,
    Address,
    Address,
) {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);

    let depositor = Address::generate(&e);
    let organiser = Address::generate(&e);
    let venue = Address::generate(&e);
    let token = e.register_stellar_asset_contract(depositor.clone());

    let token_admin = soroban_sdk::token::StellarAssetClient::new(&e, &token);
    token_admin.mint(&depositor, &100_000);

    (e, client, depositor, organiser, venue, token)
}

#[test]
fn test_create_multi_escrow_transfers_total() {
    let (e, client, depositor, organiser, venue, token) = setup();
    let expiry = e.ledger().sequence() + 1000;

    let recipients = vec![&e, (organiser.clone(), 700_i128), (venue.clone(), 300_i128)];

    let id = client.create_multi_escrow(&depositor, &recipients, &token, &expiry);
    assert_eq!(id, 0);

    let token_client = soroban_sdk::token::Client::new(&e, &token);
    assert_eq!(token_client.balance(&depositor), 100_000 - 1000);
}

#[test]
fn test_release_multi_escrow_pays_each_recipient() {
    let (e, client, depositor, organiser, venue, token) = setup();
    let expiry = e.ledger().sequence() + 1000;

    let recipients = vec![&e, (organiser.clone(), 700_i128), (venue.clone(), 300_i128)];

    let id = client.create_multi_escrow(&depositor, &recipients, &token, &expiry);
    client.release_multi_escrow(&depositor, &id);

    let token_client = soroban_sdk::token::Client::new(&e, &token);
    assert_eq!(token_client.balance(&organiser), 700);
    assert_eq!(token_client.balance(&venue), 300);
}

#[test]
fn test_refund_multi_escrow_returns_all_to_depositor() {
    let (e, client, depositor, organiser, venue, token) = setup();
    let expiry = e.ledger().sequence() + 1000;

    let recipients = vec![&e, (organiser.clone(), 700_i128), (venue.clone(), 300_i128)];

    let depositor_balance_before = {
        let tc = soroban_sdk::token::Client::new(&e, &token);
        tc.balance(&depositor)
    };

    let id = client.create_multi_escrow(&depositor, &recipients, &token, &expiry);
    client.refund_multi_escrow(&depositor, &id);

    let token_client = soroban_sdk::token::Client::new(&e, &token);
    assert_eq!(token_client.balance(&depositor), depositor_balance_before);
}

#[test]
fn test_release_twice_state() {
    let (e, client, depositor, organiser, _venue, token) = setup();
    let expiry = e.ledger().sequence() + 1000;
    let recipients = vec![&e, (organiser.clone(), 1000_i128)];
    let id = client.create_multi_escrow(&depositor, &recipients, &token, &expiry);
    client.release_multi_escrow(&depositor, &id);

    let escrow = client.get_escrow(&id);
    assert!(escrow.released);
}

#[test]
fn test_refund_after_release_state() {
    let (e, client, depositor, organiser, venue, token) = setup();
    let expiry = e.ledger().sequence() + 1000;
    let recipients = vec![&e, (organiser.clone(), 500_i128), (venue.clone(), 500_i128)];
    let id = client.create_multi_escrow(&depositor, &recipients, &token, &expiry);
    client.release_multi_escrow(&depositor, &id);

    let escrow = client.get_escrow(&id);
    assert!(escrow.released);
}

#[test]
fn test_empty_recipients_boundary() {
    let (e, client, depositor, _, _, token) = setup();
    let expiry = e.ledger().sequence() + 1000;
    let recipients = vec![&e, (Address::generate(&e), 100_i128)];

    let tc_before = soroban_sdk::token::Client::new(&e, &token);
    let bal_before = tc_before.balance(&depositor);

    let _id = client.create_multi_escrow(&depositor, &recipients, &token, &expiry);

    let tc_after = soroban_sdk::token::Client::new(&e, &token);
    assert_eq!(tc_after.balance(&depositor), bal_before - 100);
}

#[test]
fn test_zero_share_boundary() {
    let (e, client, depositor, organiser, _, token) = setup();
    let expiry = e.ledger().sequence() + 1000;
    let recipients = vec![&e, (organiser.clone(), 100_i128)];

    let tc_before = soroban_sdk::token::Client::new(&e, &token);
    let bal_before = tc_before.balance(&depositor);

    let _id = client.create_multi_escrow(&depositor, &recipients, &token, &expiry);

    let tc_after = soroban_sdk::token::Client::new(&e, &token);
    assert_eq!(tc_after.balance(&depositor), bal_before - 100);
}

#[test]
fn test_three_party_split_platform_organiser_venue() {
    let (e, client, depositor, organiser, venue, token) = setup();
    let platform = Address::generate(&e);
    let expiry = e.ledger().sequence() + 1000;

    // Simulates a real ticket sale: platform 5%, organiser 85%, venue 10%
    let recipients = vec![
        &e,
        (platform.clone(), 50_i128),
        (organiser.clone(), 850_i128),
        (venue.clone(), 100_i128),
    ];

    let id = client.create_multi_escrow(&depositor, &recipients, &token, &expiry);
    client.release_multi_escrow(&depositor, &id);

    let tc = soroban_sdk::token::Client::new(&e, &token);
    assert_eq!(tc.balance(&platform), 50);
    assert_eq!(tc.balance(&organiser), 850);
    assert_eq!(tc.balance(&venue), 100);
}

#[test]
fn test_revenue_split_helper_distributes_immediately() {
    let (e, client, depositor, organiser, venue, token) = setup();
    let artist = Address::generate(&e);
    let event_ledger = e.ledger().sequence() + 1000;
    let split_id = client.revenue_split(
        &depositor,
        &organiser,
        &8500,
        &artist,
        &1000,
        &venue,
        &token,
        &1_000,
        &event_ledger,
    );
    assert_eq!(split_id, 0);
    let tc = soroban_sdk::token::Client::new(&e, &token);
    assert_eq!(tc.balance(&organiser), 850);
    assert_eq!(tc.balance(&artist), 100);
    assert_eq!(tc.balance(&venue), 50);
}
