//! Tests for multi-beneficiary escrows: the sum-on-creation invariant, the
//! atomic release that drains the record to zero, and the refund path.
//!
//! Each entry point is exercised through a contract client so every call is a
//! fresh host invocation with its own authorization frame.

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{contract, contractimpl, vec, Address, Env, Vec};

use crate::multi_escrow;
use crate::storage_types::{MultiEscrowRecord, MultiEscrowStatus};

/// Harness contract exposing the multi-escrow entry points so tests can drive
/// them through the generated client.
#[contract]
pub struct MultiEscrowHarness;

#[contractimpl]
impl MultiEscrowHarness {
    pub fn create(
        e: Env,
        depositor: Address,
        token: Address,
        beneficiaries: Vec<Address>,
        amounts: Vec<i128>,
        total: i128,
    ) -> u64 {
        multi_escrow::create(&e, &depositor, &token, beneficiaries, amounts, total)
    }

    pub fn get(e: Env, id: u64) -> MultiEscrowRecord {
        multi_escrow::record(&e, id)
    }

    pub fn is_settled(e: Env, id: u64) -> bool {
        multi_escrow::is_settled(&e, id)
    }

    pub fn release(e: Env, caller: Address, id: u64) {
        multi_escrow::release(&e, &caller, id);
    }

    pub fn refund(e: Env, caller: Address, id: u64) {
        multi_escrow::refund(&e, &caller, id);
    }
}

fn setup() -> (Env, Address, Address, Vec<Address>, Vec<i128>) {
    let e = Env::default();
    e.mock_all_auths();
    let depositor = Address::generate(&e);
    let first = Address::generate(&e);
    let second = Address::generate(&e);
    // Clone before moving into the Vec so `first` can still be returned (#1263).
    let beneficiaries = vec![&e, first.clone(), second.clone()];
    let amounts = vec![&e, 400i128, 600i128];
    (e, depositor, first, beneficiaries, amounts)
}

fn panics<R>(f: impl FnOnce() -> R) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err()
}

#[test]
fn create_stores_the_record_and_returns_id_zero() {
    let (e, depositor, _first, beneficiaries, amounts) = setup();
    let cid = e.register(MultiEscrowHarness, ());
    let client = MultiEscrowHarnessClient::new(&e, &cid);

    let id = client.create(
        &depositor,
        &Address::generate(&e),
        &beneficiaries,
        &amounts,
        &1_000,
    );

    assert_eq!(id, 0);
    let record = client.get(&id);
    assert_eq!(record.depositor, depositor);
    assert_eq!(record.total, 1_000);
    assert_eq!(record.status, MultiEscrowStatus::Active);
    assert!(!client.is_settled(&id));
}

#[test]
#[should_panic(expected = "MultiEscrowSumMismatch")]
fn amounts_not_summing_to_the_deposit_panic() {
    let (e, depositor, _first, beneficiaries, _amounts) = setup();
    let cid = e.register(MultiEscrowHarness, ());
    let client = MultiEscrowHarnessClient::new(&e, &cid);
    // amounts sum to 900 but the declared total is 1_000.
    let bad_amounts = vec![&e, 400i128, 500i128];

    let _ = client.create(
        &depositor,
        &Address::generate(&e),
        &beneficiaries,
        &bad_amounts,
        &1_000,
    );
}

#[test]
#[should_panic(expected = "MultiEscrowSumMismatch")]
fn amounts_summing_above_the_deposit_panic() {
    let (e, depositor, _first, beneficiaries, _amounts) = setup();
    let cid = e.register(MultiEscrowHarness, ());
    let client = MultiEscrowHarnessClient::new(&e, &cid);
    // amounts sum to 1_200 but the declared total is 1_000.
    let amounts = vec![&e, 600i128, 600i128];

    let _ = client.create(
        &depositor,
        &Address::generate(&e),
        &beneficiaries,
        &amounts,
        &1_000,
    );
}

#[test]
#[should_panic(expected = "MultiEscrowLengthMismatch")]
fn mismatched_vector_lengths_panic() {
    let (e, depositor, _first, beneficiaries, _amounts) = setup();
    let cid = e.register(MultiEscrowHarness, ());
    let client = MultiEscrowHarnessClient::new(&e, &cid);
    let amounts = vec![&e, 400i128];

    let _ = client.create(
        &depositor,
        &Address::generate(&e),
        &beneficiaries,
        &amounts,
        &400,
    );
}

#[test]
fn a_valid_release_drains_the_record_to_zero() {
    let (e, depositor, _first, beneficiaries, amounts) = setup();
    let cid = e.register(MultiEscrowHarness, ());
    let client = MultiEscrowHarnessClient::new(&e, &cid);

    let id = client.create(
        &depositor,
        &Address::generate(&e),
        &beneficiaries,
        &amounts,
        &1_000,
    );

    client.release(&depositor, &id);

    let record = client.get(&id);
    assert_eq!(record.status, MultiEscrowStatus::Released);
    assert!(client.is_settled(&id));
    // The record is drained to zero: every leg was paid, nothing is owed.
    assert_eq!(record.amounts.len(), 0);
    assert_eq!(record.total, 1_000);
}

#[test]
#[should_panic(expected = "MultiEscrowNotActive")]
fn a_settled_record_cannot_be_released() {
    let (e, depositor, _first, beneficiaries, amounts) = setup();
    let cid = e.register(MultiEscrowHarness, ());
    let client = MultiEscrowHarnessClient::new(&e, &cid);

    let id = client.create(
        &depositor,
        &Address::generate(&e),
        &beneficiaries,
        &amounts,
        &1_000,
    );
    client.release(&depositor, &id);
    client.release(&depositor, &id);
}

#[test]
#[should_panic(expected = "MultiEscrowNotActive")]
fn a_released_record_cannot_be_refunded() {
    let (e, depositor, _first, beneficiaries, amounts) = setup();
    let cid = e.register(MultiEscrowHarness, ());
    let client = MultiEscrowHarnessClient::new(&e, &cid);

    let id = client.create(
        &depositor,
        &Address::generate(&e),
        &beneficiaries,
        &amounts,
        &1_000,
    );
    client.release(&depositor, &id);
    client.refund(&depositor, &id);
}

#[test]
fn a_stranger_cannot_release() {
    let (e, depositor, _first, beneficiaries, amounts) = setup();
    let cid = e.register(MultiEscrowHarness, ());
    let client = MultiEscrowHarnessClient::new(&e, &cid);
    let stranger = Address::generate(&e);

    let id = client.create(
        &depositor,
        &Address::generate(&e),
        &beneficiaries,
        &amounts,
        &1_000,
    );

    assert!(panics(|| {
        client.release(&stranger, &id);
    }));
    assert!(!client.is_settled(&id));
}

#[test]
#[should_panic(expected = "MultiEscrowNotFound")]
fn get_panics_on_an_unknown_id() {
    let (e, _depositor, _first, _beneficiaries, _amounts) = setup();
    let cid = e.register(MultiEscrowHarness, ());
    let client = MultiEscrowHarnessClient::new(&e, &cid);

    let _ = client.get(&42);
}

#[test]
#[should_panic(expected = "MultiEscrowNotFound")]
fn release_panics_on_an_unknown_id() {
    let (e, depositor, _first, _beneficiaries, _amounts) = setup();
    let cid = e.register(MultiEscrowHarness, ());
    let client = MultiEscrowHarnessClient::new(&e, &cid);

    client.release(&depositor, &42);
}

#[test]
#[should_panic(expected = "InvalidAmount")]
fn negative_amount_panics() {
    let (e, depositor, _first, beneficiaries, _amounts) = setup();
    let cid = e.register(MultiEscrowHarness, ());
    let client = MultiEscrowHarnessClient::new(&e, &cid);
    let bad_amounts = vec![&e, 1_400i128, -400i128];

    let _ = client.create(
        &depositor,
        &Address::generate(&e),
        &beneficiaries,
        &bad_amounts,
        &1_000,
    );
}

#[test]
#[should_panic(expected = "InvalidAmount")]
fn zero_total_panics() {
    let (e, depositor, _first, beneficiaries, _amounts) = setup();
    let cid = e.register(MultiEscrowHarness, ());
    let client = MultiEscrowHarnessClient::new(&e, &cid);
    let zero_amounts = vec![&e, 0i128, 0i128];

    let _ = client.create(
        &depositor,
        &Address::generate(&e),
        &beneficiaries,
        &zero_amounts,
        &0,
    );
}
