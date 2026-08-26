#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};
use crate::contract::{VeriTixPay, VeriTixPayClient};

struct TestEnv<'a> {
    e: Env,
    client: VeriTixPayClient<'a>,
    depositor: Address,
    beneficiary: Address,
    token: Address,
    arbiter: Address,
}

fn setup() -> TestEnv<'static> {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);

    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let arbiter = Address::generate(&e);
    let token = e.register_stellar_asset_contract(depositor.clone());

    soroban_sdk::token::StellarAssetClient::new(&e, &token).mint(&depositor, &50_000);
    client.set_arbiter(&arbiter);

    TestEnv { e, client, depositor, beneficiary, token, arbiter }
}

#[test]
fn test_dispute_on_one_escrow_does_not_affect_another() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    let depositor1 = t.depositor;
    let beneficiary1 = t.beneficiary;

    let depositor2 = Address::generate(&t.e);
    let beneficiary2 = Address::generate(&t.e);
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token).mint(&depositor2, &1000);

    let escrow1_id = t.client.create_escrow(
        &depositor1,
        &beneficiary1,
        &t.token,
        &100,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
    );

    let escrow2_id = t.client.create_escrow(
        &depositor2,
        &beneficiary2,
        &t.token,
        &200,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
    );

    t.client.raise_dispute(&depositor1, &escrow1_id);
    t.client.resolve_dispute(&t.arbiter, &escrow1_id, &beneficiary1);

    let escrow2 = t.client.get_escrow(&escrow2_id);
    assert!(!escrow2.released);

    t.client.release_escrow(&depositor2, &escrow2_id);

    let tc = soroban_sdk::token::Client::new(&t.e, &t.token);
    assert_eq!(tc.balance(&beneficiary1), 100);
    assert_eq!(tc.balance(&beneficiary2), 200);
}

// ── #453: dispute_resolution_stats ────────────────────────────────────────────

#[test]
fn test_resolver_stats_zero_for_unknown_address() {
    let t = setup();
    let unknown = Address::generate(&t.e);
    let stats = t.client.resolver_stats(&unknown);
    assert_eq!(stats.total_resolved, 0);
    assert_eq!(stats.for_beneficiary, 0);
    assert_eq!(stats.for_depositor, 0);
}

#[test]
fn test_resolver_stats_track_resolution_for_beneficiary() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    let dep2 = Address::generate(&t.e);
    let ben2 = Address::generate(&t.e);
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token).mint(&dep2, &5_000);

    let escrow_id = t.client.create_escrow(
        &dep2, &ben2, &t.token, &500, &expiry, &crate::escrow_test::empty_memo(&t.e),
    );

    t.client.raise_dispute(&dep2, &escrow_id);
    t.client.resolve_dispute(&t.arbiter, &escrow_id, &ben2);

    let stats = t.client.resolver_stats(&t.arbiter);
    assert_eq!(stats.total_resolved, 1);
    assert_eq!(stats.for_beneficiary, 1);
    assert_eq!(stats.for_depositor, 0);
}

#[test]
fn test_resolver_stats_track_resolution_for_depositor() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    let dep2 = Address::generate(&t.e);
    let ben2 = Address::generate(&t.e);
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token).mint(&dep2, &5_000);

    let escrow_id = t.client.create_escrow(
        &dep2, &ben2, &t.token, &500, &expiry, &crate::escrow_test::empty_memo(&t.e),
    );

    t.client.raise_dispute(&dep2, &escrow_id);
    t.client.resolve_dispute(&t.arbiter, &escrow_id, &dep2);

    let stats = t.client.resolver_stats(&t.arbiter);
    assert_eq!(stats.total_resolved, 1);
    assert_eq!(stats.for_beneficiary, 0);
    assert_eq!(stats.for_depositor, 1);
}

#[test]
fn test_resolver_stats_accumulate_across_resolutions() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    let dep2 = Address::generate(&t.e);
    let ben2 = Address::generate(&t.e);
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token).mint(&dep2, &10_000);

    // First dispute resolved for beneficiary
    let id1 = t.client.create_escrow(
        &dep2, &ben2, &t.token, &500, &expiry, &crate::escrow_test::empty_memo(&t.e),
    );
    t.client.raise_dispute(&dep2, &id1);
    t.client.resolve_dispute(&t.arbiter, &id1, &ben2);

    // Second dispute resolved for depositor
    let id2 = t.client.create_escrow(
        &dep2, &ben2, &t.token, &500, &expiry, &crate::escrow_test::empty_memo(&t.e),
    );
    t.client.raise_dispute(&dep2, &id2);
    t.client.resolve_dispute(&t.arbiter, &id2, &dep2);

    let stats = t.client.resolver_stats(&t.arbiter);
    assert_eq!(stats.total_resolved, 2);
    assert_eq!(stats.for_beneficiary, 1);
    assert_eq!(stats.for_depositor, 1);
}

#[test]
#[should_panic(expected = "only depositor or beneficiary can open dispute")]
fn test_open_dispute_unauthorized_party_panics() {
    let t = setup();
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token).mint(&t.depositor, &10_000_000);
    let stranger = Address::generate(&t.e);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor, &t.beneficiary, &t.token, &10_000_000, &expiry, &crate::escrow_test::empty_memo(&t.e),
    );

    t.e.as_contract(&t.client.address, || {
        crate::dispute::open_dispute(t.e.clone(), stranger, id, 1);
    });
}
