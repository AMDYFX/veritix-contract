//! Tests for the dispute lifecycle: raising, the read views, the arbiter's
//! ruling and appeal window, escalation, and the final appeal ruling.
//!
//! Each entry point is exercised through a contract client so every call is a
//! fresh host invocation with its own authorization frame; storage seeding is
//! done directly inside `as_contract`.

use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};
use soroban_sdk::{contract, contractimpl, Address, Env};

use crate::balance;
use crate::dispute::APPEAL_WINDOW_LEDGERS;
use crate::storage_types::{DataKey, DisputeRecord, DisputeStatus, EscrowRecord, EscrowStatus};

/// Harness contract exposing the dispute entry points so tests can drive them
/// through the generated client (self-contained mirror of `contract.rs`).
#[contract]
pub struct DisputeHarness;

#[contractimpl]
impl DisputeHarness {
    pub fn raise_dispute(e: Env, claimant: Address, escrow_id: u32, resolver: Option<Address>) {
        crate::dispute::raise_dispute(&e, &claimant, escrow_id, resolver);
    }

    pub fn is_dispute_open(e: Env, escrow_id: u32) -> bool {
        crate::dispute::is_dispute_open(&e, escrow_id)
    }

    pub fn get_dispute(e: Env, escrow_id: u32) -> DisputeRecord {
        crate::dispute::get_dispute(&e, escrow_id)
    }

    pub fn resolve_dispute(e: Env, resolver: Address, escrow_id: u32, winner: Address) {
        crate::dispute::resolve_dispute(&e, resolver, escrow_id, winner);
    }

    pub fn appeal_dispute(e: Env, caller: Address, escrow_id: u32) {
        crate::dispute::appeal_dispute(&e, &caller, escrow_id);
    }

    pub fn resolve_appeal(e: Env, resolver: Address, escrow_id: u32, winner: Address) {
        crate::dispute::resolve_appeal(&e, resolver, escrow_id, winner);
    }

    pub fn expire_dispute(e: Env, caller: Address, escrow_id: u32) {
        crate::dispute::expire_dispute(&e, &caller, escrow_id);
    }
}

fn escrow(e: &Env, depositor: &Address, beneficiary: &Address, amount: i128) -> EscrowRecord {
    EscrowRecord {
        depositor: depositor.clone(),
        beneficiary: beneficiary.clone(),
        token: Address::generate(e),
        amount,
        deadline_ledger: e.ledger().sequence() + 1_000,
        status: EscrowStatus::Active,
        created_ledger: e.ledger().sequence(),
    }
}

/// Stores an escrow and its depositor's balance lock directly, mirroring how
/// a raised escrow records both before a dispute can reference it.
fn seed_escrow(e: &Env, cid: &Address, id: u32, record: &EscrowRecord, locked: i128) {
    e.as_contract(cid, || {
        e.storage()
            .persistent()
            .set(&DataKey::EscrowRecord(id), record);
        e.storage()
            .persistent()
            .set(&DataKey::EscrowLocked(record.depositor.clone()), &locked);
    });
}

fn credit(e: &Env, cid: &Address, account: &Address, amount: i128) {
    e.as_contract(cid, || {
        balance::credit(e, account, amount);
    });
}

fn balance_of(e: &Env, cid: &Address, account: &Address) -> i128 {
    e.as_contract(cid, || balance::balance_of(e, account))
}

fn escrow_locked(e: &Env, cid: &Address, account: &Address) -> i128 {
    e.as_contract(cid, || {
        e.storage()
            .persistent()
            .get(&DataKey::EscrowLocked(account.clone()))
            .unwrap_or(0)
    })
}

fn escrow_status(e: &Env, cid: &Address, id: u32) -> EscrowStatus {
    e.as_contract(cid, || {
        e.storage()
            .persistent()
            .get::<_, EscrowRecord>(&DataKey::EscrowRecord(id))
            .unwrap()
            .status
    })
}

fn setup() -> (Env, Address, Address, Address, Address, u32) {
    let e = Env::default();
    e.mock_all_auths();
    let cid = e.register_contract(None, DisputeHarness);
    let resolver = Address::generate(&e);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    e.ledger().set(LedgerInfo {
        protocol_version: 28,
        sequence_number: 1000,
        ..Default::default()
    });
    (e, cid, resolver, depositor, beneficiary, 7)
}

fn jump(e: &Env, sequence: u32) {
    e.ledger().set(LedgerInfo {
        protocol_version: 28,
        sequence_number: sequence,
        ..Default::default()
    });
}

fn seed_full(
    e: &Env,
    cid: &Address,
    client: &DisputeHarnessClient<'_>,
    resolver: &Address,
    depositor: &Address,
    beneficiary: &Address,
    id: u32,
) {
    seed_escrow(e, cid, id, &escrow(e, depositor, beneficiary, 100), 100);
    credit(e, cid, depositor, 100);
    client.raise_dispute(depositor, &id, &Some(resolver.clone()));
}

#[test]
fn raise_dispute_opens_and_views_agree() {
    let (e, cid, _resolver, depositor, beneficiary, id) = setup();
    let client = DisputeHarnessClient::new(&e, &cid);
    seed_escrow(
        &e,
        &cid,
        id,
        &escrow(&e, &depositor, &beneficiary, 100),
        100,
    );

    client.raise_dispute(&depositor, &id, &None);

    assert!(client.is_dispute_open(&id));
    let record = client.get_dispute(&id);
    assert_eq!(record.claimant, depositor);
    assert_eq!(record.status, DisputeStatus::Open);
    assert_eq!(record.winner, None);
    assert!(!record.is_final);
}

#[test]
fn is_dispute_open_is_false_without_a_dispute() {
    let (e, _cid, _r, _d, _b, _id) = setup();
    let client = DisputeHarnessClient::new(&e, &_cid);
    assert!(!client.is_dispute_open(&99));
}

#[test]
#[should_panic(expected = "DisputeAlreadyOpen")]
fn raise_dispute_twice_rejects() {
    let (e, cid, _resolver, depositor, beneficiary, id) = setup();
    let client = DisputeHarnessClient::new(&e, &cid);
    seed_escrow(
        &e,
        &cid,
        id,
        &escrow(&e, &depositor, &beneficiary, 100),
        100,
    );

    client.raise_dispute(&depositor, &id, &None);
    client.raise_dispute(&depositor, &id, &None);
}

#[test]
#[should_panic(expected = "DisputeNotFound")]
fn get_dispute_rejects_unknown_escrow() {
    let (e, _cid, _r, _d, _b, _id) = setup();
    let client = DisputeHarnessClient::new(&e, &_cid);
    client.get_dispute(&42);
}

#[test]
fn resolve_dispute_records_ruling_and_blocks_payout_until_window_lapses() {
    let (e, cid, resolver, depositor, beneficiary, id) = setup();
    let client = DisputeHarnessClient::new(&e, &cid);
    seed_full(&e, &cid, &client, &resolver, &depositor, &beneficiary, id);

    client.resolve_dispute(&resolver, &id, &beneficiary);

    let record = client.get_dispute(&id);
    assert_eq!(record.status, DisputeStatus::Resolved);
    assert_eq!(record.winner, Some(beneficiary.clone()));
    assert!(!record.is_final);

    // Payout is blocked: nothing moved, the escrow is still locked.
    // The escrow was marked `Disputed` on raise and stays frozen through the
    // first ruling; only a final ruling settles it.
    assert_eq!(balance_of(&e, &cid, &beneficiary), 0);
    assert_eq!(escrow_locked(&e, &cid, &depositor), 100);
    assert_eq!(escrow_status(&e, &cid, id), EscrowStatus::Disputed);
}

#[test]
#[should_panic(expected = "AppealWindowOpen")]
fn resolve_dispute_cannot_finalize_before_the_window_lapses() {
    let (e, cid, resolver, depositor, beneficiary, id) = setup();
    let client = DisputeHarnessClient::new(&e, &cid);
    seed_full(&e, &cid, &client, &resolver, &depositor, &beneficiary, id);

    client.resolve_dispute(&resolver, &id, &beneficiary);
    // Same ledger: still inside the window.
    client.resolve_dispute(&resolver, &id, &beneficiary);
}

#[test]
fn resolve_dispute_finalizes_after_the_window_and_pays_the_winner() {
    let (e, cid, resolver, depositor, beneficiary, id) = setup();
    let client = DisputeHarnessClient::new(&e, &cid);
    seed_full(&e, &cid, &client, &resolver, &depositor, &beneficiary, id);

    client.resolve_dispute(&resolver, &id, &beneficiary);
    let ruling = e.ledger().sequence();
    jump(&e, ruling + APPEAL_WINDOW_LEDGERS + 1);

    client.resolve_dispute(&resolver, &id, &beneficiary);

    let record = client.get_dispute(&id);
    assert!(record.is_final);
    assert_eq!(record.winner, Some(beneficiary.clone()));
    assert!(!client.is_dispute_open(&id));
    assert_eq!(balance_of(&e, &cid, &beneficiary), 100);
    assert_eq!(escrow_locked(&e, &cid, &depositor), 0);
    assert_eq!(escrow_status(&e, &cid, id), EscrowStatus::Released);
}

#[test]
#[should_panic(expected = "DisputeAlreadyOpen")]
fn raise_dispute_after_final_resolution_is_rejected() {
    let (e, cid, resolver, depositor, beneficiary, id) = setup();
    let client = DisputeHarnessClient::new(&e, &cid);
    seed_full(&e, &cid, &client, &resolver, &depositor, &beneficiary, id);

    client.resolve_dispute(&resolver, &id, &beneficiary);
    let ruling = e.ledger().sequence();
    jump(&e, ruling + APPEAL_WINDOW_LEDGERS + 1);
    client.resolve_dispute(&resolver, &id, &beneficiary);

    client.raise_dispute(&depositor, &id, &None);
}

#[test]
#[should_panic(expected = "DisputeFinal")]
fn resolve_dispute_after_final_resolution_is_rejected() {
    let (e, cid, resolver, depositor, beneficiary, id) = setup();
    let client = DisputeHarnessClient::new(&e, &cid);
    seed_full(&e, &cid, &client, &resolver, &depositor, &beneficiary, id);

    client.resolve_dispute(&resolver, &id, &beneficiary);
    let ruling = e.ledger().sequence();
    jump(&e, ruling + APPEAL_WINDOW_LEDGERS + 1);
    client.resolve_dispute(&resolver, &id, &beneficiary);

    client.resolve_dispute(&resolver, &id, &beneficiary);
}

#[test]
#[should_panic(expected = "DisputeNotOpen")]
fn expired_dispute_cannot_be_resolved() {
    let (e, cid, resolver, depositor, beneficiary, id) = setup();
    let client = DisputeHarnessClient::new(&e, &cid);
    seed_full(&e, &cid, &client, &resolver, &depositor, &beneficiary, id);
    e.as_contract(&cid, || {
        let mut record = e
            .storage()
            .persistent()
            .get::<_, DisputeRecord>(&DataKey::Dispute(id))
            .unwrap();
        record.status = DisputeStatus::Expired;
        e.storage().persistent().set(&DataKey::Dispute(id), &record);
    });

    client.resolve_dispute(&resolver, &id, &beneficiary);
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not the assigned resolver")]
fn resolve_dispute_rejects_a_stranger_resolver() {
    let (e, cid, _resolver, depositor, beneficiary, id) = setup();
    let client = DisputeHarnessClient::new(&e, &cid);
    seed_escrow(
        &e,
        &cid,
        id,
        &escrow(&e, &depositor, &beneficiary, 100),
        100,
    );
    client.raise_dispute(&depositor, &id, &Some(Address::generate(&e)));

    client.resolve_dispute(&Address::generate(&e), &id, &beneficiary);
}

#[test]
#[should_panic(expected = "WinnerNotEscrowParty")]
fn resolve_dispute_rejects_a_non_party_winner() {
    let (e, cid, resolver, depositor, beneficiary, id) = setup();
    let client = DisputeHarnessClient::new(&e, &cid);
    seed_full(&e, &cid, &client, &resolver, &depositor, &beneficiary, id);

    client.resolve_dispute(&resolver, &id, &Address::generate(&e));
}

#[test]
#[should_panic(expected = "DisputeFinal")]
fn appeal_after_a_final_ruling_is_rejected() {
    let (e, cid, resolver, depositor, beneficiary, id) = setup();
    let client = DisputeHarnessClient::new(&e, &cid);
    seed_full(&e, &cid, &client, &resolver, &depositor, &beneficiary, id);

    client.resolve_dispute(&resolver, &id, &beneficiary);
    let ruling = e.ledger().sequence();
    jump(&e, ruling + APPEAL_WINDOW_LEDGERS + 1);
    client.resolve_dispute(&resolver, &id, &beneficiary);

    client.appeal_dispute(&depositor, &id);
}

#[test]
fn appeal_reopens_and_resolve_appeal_is_final() {
    let (e, cid, resolver, depositor, beneficiary, id) = setup();
    let client = DisputeHarnessClient::new(&e, &cid);
    seed_full(&e, &cid, &client, &resolver, &depositor, &beneficiary, id);

    client.resolve_dispute(&resolver, &id, &beneficiary);
    client.appeal_dispute(&depositor, &id);

    let record = client.get_dispute(&id);
    assert_eq!(record.status, DisputeStatus::Appealed);
    assert!(client.is_dispute_open(&id));
    assert_eq!(balance_of(&e, &cid, &beneficiary), 0);

    client.resolve_appeal(&resolver, &id, &depositor);

    let record = client.get_dispute(&id);
    assert_eq!(record.status, DisputeStatus::Resolved);
    assert!(record.is_final);
    assert_eq!(record.winner, Some(depositor.clone()));
    assert!(!client.is_dispute_open(&id));
    assert_eq!(escrow_locked(&e, &cid, &depositor), 0);
    assert_eq!(escrow_status(&e, &cid, id), EscrowStatus::Refunded);
}

#[test]
#[should_panic(expected = "AppealWindowClosed")]
fn appeal_outside_the_window_is_rejected() {
    let (e, cid, resolver, depositor, beneficiary, id) = setup();
    let client = DisputeHarnessClient::new(&e, &cid);
    seed_full(&e, &cid, &client, &resolver, &depositor, &beneficiary, id);

    client.resolve_dispute(&resolver, &id, &beneficiary);
    let ruling = e.ledger().sequence();
    jump(&e, ruling + APPEAL_WINDOW_LEDGERS + 1);

    client.appeal_dispute(&depositor, &id);
}

#[test]
#[should_panic(expected = "AppealCallerNotParty")]
fn the_declared_winner_cannot_appeal() {
    let (e, cid, resolver, depositor, beneficiary, id) = setup();
    let client = DisputeHarnessClient::new(&e, &cid);
    seed_full(&e, &cid, &client, &resolver, &depositor, &beneficiary, id);

    client.resolve_dispute(&resolver, &id, &beneficiary);

    client.appeal_dispute(&beneficiary, &id);
}

#[test]
#[should_panic(expected = "DisputeNotAppealed")]
fn resolve_appeal_requires_an_appeal() {
    let (e, cid, resolver, depositor, beneficiary, id) = setup();
    let client = DisputeHarnessClient::new(&e, &cid);
    seed_full(&e, &cid, &client, &resolver, &depositor, &beneficiary, id);

    client.resolve_dispute(&resolver, &id, &beneficiary);

    client.resolve_appeal(&resolver, &id, &beneficiary);
}

#[test]
#[should_panic(expected = "DisputeSelfArbitration")]
fn party_cannot_arbitrate_own_dispute() {
    let (e, cid, _resolver, depositor, beneficiary, id) = setup();
    let client = DisputeHarnessClient::new(&e, &cid);
    seed_escrow(
        &e,
        &cid,
        id,
        &escrow(&e, &depositor, &beneficiary, 100),
        100,
    );
    client.raise_dispute(&depositor, &id, &Some(depositor.clone()));
}

#[test]
#[should_panic(expected = "WinnerMismatch")]
fn winner_cannot_be_changed_at_finalisation() {
    let (e, cid, resolver, depositor, beneficiary, id) = setup();
    let client = DisputeHarnessClient::new(&e, &cid);
    seed_full(&e, &cid, &client, &resolver, &depositor, &beneficiary, id);

    client.resolve_dispute(&resolver, &id, &beneficiary);
    let ruling = e.ledger().sequence();
    jump(&e, ruling + APPEAL_WINDOW_LEDGERS + 1);

    client.resolve_dispute(&resolver, &id, &depositor);
}
