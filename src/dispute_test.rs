#![cfg(test)]

use crate::contract::{VeriTixPay, VeriTixPayClient};
use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Address, Env};

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

    TestEnv {
        e,
        client,
        depositor,
        beneficiary,
        token,
        arbiter,
    }
}

#[test]
fn test_dispute_on_one_escrow_does_not_affect_another() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    let depositor1 = t.depositor;
    let beneficiary1 = t.beneficiary;
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token).mint(&depositor1, &10_000_000);

    let depositor2 = Address::generate(&t.e);
    let beneficiary2 = Address::generate(&t.e);
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token).mint(&depositor2, &10_000_000);

    let escrow1_id = t.client.create_escrow(
        &depositor1,
        &beneficiary1,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
    );

    let escrow2_id = t.client.create_escrow(
        &depositor2,
        &beneficiary2,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
    );

    t.client.raise_dispute(&depositor1, &escrow1_id);
    t.client
        .resolve_dispute(&t.arbiter, &escrow1_id, &beneficiary1);

    let escrow2 = t.client.get_escrow(&escrow2_id);
    assert!(!escrow2.released);

    t.client.release_escrow(&depositor2, &escrow2_id);

    let tc = soroban_sdk::token::Client::new(&t.e, &t.token);
    assert_eq!(
        tc.balance(&beneficiary1),
        crate::storage_types::MIN_ESCROW_AMOUNT
    );
    assert_eq!(
        tc.balance(&beneficiary2),
        crate::storage_types::MIN_ESCROW_AMOUNT
    );
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
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token)
        .mint(&dep2, &crate::storage_types::MIN_ESCROW_AMOUNT);

    let escrow_id = t.client.create_escrow(
        &dep2,
        &ben2,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
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
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token)
        .mint(&dep2, &crate::storage_types::MIN_ESCROW_AMOUNT);

    let escrow_id = t.client.create_escrow(
        &dep2,
        &ben2,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
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
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token)
        .mint(&dep2, &(2 * crate::storage_types::MIN_ESCROW_AMOUNT));

    // First dispute resolved for beneficiary
    let id1 = t.client.create_escrow(
        &dep2,
        &ben2,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
    );
    t.client.raise_dispute(&dep2, &id1);
    t.client.resolve_dispute(&t.arbiter, &id1, &ben2);

    // Second dispute resolved for depositor
    let id2 = t.client.create_escrow(
        &dep2,
        &ben2,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
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
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &10_000_000,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
    );

    t.e.as_contract(&t.client.address, || {
        crate::dispute::open_dispute(t.e.clone(), stranger, id, 1);
    });
}

// ── #669: claimant validation ─────────────────────────────────────────────────

#[test]
#[should_panic(expected = "only depositor or beneficiary can raise dispute")]
fn test_raise_dispute_by_non_party_panics() {
    let t = setup();
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token).mint(&t.depositor, &10_000_000);
    let stranger = Address::generate(&t.e);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
    );
    t.client.raise_dispute(&stranger, &id);
}

#[test]
fn test_raise_dispute_by_depositor_succeeds() {
    let t = setup();
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token).mint(&t.depositor, &10_000_000);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
    );
    t.client.raise_dispute(&t.depositor, &id);
    // The dispute was raised successfully; resolving it confirms the dispute existed.
    t.client.resolve_dispute(&t.arbiter, &id, &t.depositor);
    assert!(t.client.get_escrow(&id).refunded);
}

#[test]
fn test_raise_dispute_by_beneficiary_succeeds() {
    let t = setup();
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token).mint(&t.depositor, &10_000_000);
    let expiry = t.e.ledger().sequence() + 1000;
// ── #695: is_dispute_open ─────────────────────────────────────────────────────

#[test]
fn test_is_dispute_open_returns_true_when_dispute_is_open() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token).mint(&t.depositor, &10_000_000);

    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
    );
    t.client.raise_dispute(&t.beneficiary, &id);
    t.client.resolve_dispute(&t.arbiter, &id, &t.beneficiary);
    assert!(t.client.get_escrow(&id).released);
}

// ── #670: appeal and resolve_appeal ───────────────────────────────────────────

#[test]
fn test_appeal_dispute_within_window_succeeds() {
    let t = setup();
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token).mint(&t.depositor, &10_000_000);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
    );
    t.client.raise_dispute(&t.depositor, &id);

    t.e.as_contract(&t.client.address, || {
        crate::dispute::appeal_dispute(&t.e, &t.depositor, id);
    });
}

#[test]
fn test_appeal_then_resolve_appeal_settles() {
    let t = setup();
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token).mint(&t.depositor, &10_000_000);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
    );
    t.client.raise_dispute(&t.depositor, &id);

    t.e.as_contract(&t.client.address, || {
        crate::dispute::appeal_dispute(&t.e, &t.depositor, id);
    });

    t.client.resolve_appeal(&t.arbiter, &id, &t.depositor);
    let record = t.client.get_escrow(&id);
    assert!(record.refunded);
    assert_eq!(t.client.resolver_stats(&t.arbiter).total_resolved, 1);
}

#[test]
#[should_panic(expected = "appeal window has expired")]
fn test_appeal_after_window_panics() {
    let t = setup();
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token).mint(&t.depositor, &10_000_000);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
    );
    let opened_at = t.e.ledger().sequence();
    t.client.raise_dispute(&t.depositor, &id);

    t.e.ledger()
        .with_mut(|l| l.sequence_number = opened_at + crate::dispute::DISPUTE_APPEAL_WINDOW + 1);
    t.e.as_contract(&t.client.address, || {
        crate::dispute::appeal_dispute(&t.e, &t.depositor, id);
    });
}

#[test]
#[should_panic(expected = "no pending appeal to resolve")]
fn test_resolve_appeal_without_appeal_panics() {
    let t = setup();
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token).mint(&t.depositor, &10_000_000);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
    );
    t.client.raise_dispute(&t.depositor, &id);
    t.client.resolve_appeal(&t.arbiter, &id, &t.depositor);
}

// ── #671: expire_dispute ──────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "dispute has not been open long enough to expire")]
fn test_expire_dispute_before_window_panics() {
    let t = setup();
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token).mint(&t.depositor, &10_000_000);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
    );
    t.client.raise_dispute(&t.depositor, &id);
    t.client.expire_dispute(&t.depositor, &id);
}

#[test]
fn test_expire_dispute_after_window_releases_escrow() {
    let t = setup();
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token).mint(&t.depositor, &10_000_000);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
    );
    let opened_at = t.e.ledger().sequence();
    t.client.raise_dispute(&t.depositor, &id);

    t.e.ledger()
        .with_mut(|l| l.sequence_number = opened_at + crate::dispute::DISPUTE_EXPIRE_AFTER + 1);
    t.client.expire_dispute(&t.depositor, &id);

    // After expiry, refund becomes possible again.
    t.client.refund_escrow(&t.depositor, &id);
    assert!(t.client.get_escrow(&id).refunded);
}

#[test]
#[should_panic(expected = "escrow is not under dispute")]
fn test_expire_dispute_not_disputed_panics() {
    let t = setup();
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token).mint(&t.depositor, &10_000_000);
    let expiry = t.e.ledger().sequence() + 1000;
        &10_000_000,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
    );

    assert!(!t.client.is_dispute_open(&id));

    t.client.raise_dispute(&t.depositor, &id);
    assert!(t.client.is_dispute_open(&id));
}

#[test]
fn test_is_dispute_open_returns_false_after_resolution() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token).mint(&t.depositor, &10_000_000);

    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
    );
    t.client.expire_dispute(&t.depositor, &id);
}

// ── #672: dispute claimant index (portableDD) ─────────────────────────────────

#[test]
fn test_raise_dispute_populates_claimant_index() {
    let t = setup();
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token).mint(&t.depositor, &10_000_000);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
    );
    t.client.raise_dispute(&t.depositor, &id);

    t.e.as_contract(&t.client.address, || {
        let disputes = crate::dispute::get_disputes_by_claimant(t.e.clone(), t.depositor.clone());
        assert!(!disputes.is_empty());
    });
}

#[test]
fn test_resolver_stats_increment_on_resolution() {
    let t = setup();
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token).mint(&t.depositor, &10_000_000);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
    );
    t.client.raise_dispute(&t.depositor, &id);
    t.client.resolve_dispute(&t.arbiter, &id, &t.beneficiary);

    let stats = t.client.resolver_stats(&t.arbiter);
    assert_eq!(stats.total_resolved, 1);
    assert_eq!(stats.for_beneficiary, 1);
}

#[test]
fn test_dispute_index_isolation_between_claimants() {
    let t = setup();
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token).mint(&t.depositor, &10_000_000);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
    );
    t.client.raise_dispute(&t.depositor, &id);

    let other = Address::generate(&t.e);
    t.e.as_contract(&t.client.address, || {
        let disputes = crate::dispute::get_disputes_by_claimant(t.e.clone(), other);
        assert_eq!(disputes.len(), 0);
    });
        &10_000_000,
        &expiry,
        &crate::escrow_test::empty_memo(&t.e),
    );

    t.client.raise_dispute(&t.depositor, &id);
    assert!(t.client.is_dispute_open(&id));

    t.client.resolve_dispute(&t.arbiter, &id, &t.beneficiary);
    assert!(!t.client.is_dispute_open(&id));
}

#[test]
fn test_is_dispute_open_returns_false_without_dispute() {
    let t = setup();
    // Escrow that was never disputed reports false.
    assert!(!t.client.is_dispute_open(&u32::MAX));
}
