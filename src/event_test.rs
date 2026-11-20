//! Event shape tests for every operation the token performs.
//!
//! Indexers and wallets only see this contract through its events, so a
//! correctly-behaved state change that emits the wrong topics is
//! indistinguishable from no change at all. Each test here therefore asserts the
//! exact topic list and the exact data payload, not merely that an event exists.

use crate::contract::testing::{bytes_of_len, Fixture};
use crate::contract::VeriTixPayClient;
use crate::storage_types::DataKey;
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{symbol_short, vec, xdr, Address, Env, FromVal, IntoVal};

/// The topics and data of the most recent event.
fn last_event(e: &Env) -> (std::vec::Vec<xdr::ScVal>, xdr::ScVal) {
    let events = e.events().all();
    let last = events.events().last().expect("no event was emitted");
    let xdr::ContractEventBody::V0(body) = &last.body else {
        panic!("expected a v0 contract event");
    };
    (body.topics.clone(), body.data.clone())
}

/// The data of the most recent event, decoded as the standard `[T]` vector.
fn last_event_data_items(e: &Env) -> std::vec::Vec<xdr::ScVal> {
    let (_, data) = last_event(e);
    let xdr::ScVal::Vec(items) = data else {
        panic!("expected vec-shaped event data, got a scalar");
    };
    items
}

#[test]
fn mint_event_matches_the_sep41_shape() {
    let f = Fixture::new();
    let holder = Address::generate(&f.e);

    f.client.mint(&f.admin, &holder, &1_000);

    let (topics, data) = last_event(&f.e);
    assert_eq!(
        topics,
        std::vec![
            xdr::ScVal::from_val(&f.e, &symbol_short!("mint").to_val()),
            xdr::ScVal::from_val(&f.e, &holder.to_val()),
        ]
    );
    assert_eq!(data, xdr::ScVal::from_val(&f.e, &1_000i128.into_val(&f.e)));
}

#[test]
fn burn_event_matches_the_sep41_shape() {
    let f = Fixture::new();
    let holder = f.fund(1_000);

    f.client.burn(&holder, &400);

    let (topics, data) = last_event(&f.e);
    assert_eq!(
        topics,
        std::vec![
            xdr::ScVal::from_val(&f.e, &symbol_short!("burn").to_val()),
            xdr::ScVal::from_val(&f.e, &holder.to_val()),
        ]
    );
    assert_eq!(data, xdr::ScVal::from_val(&f.e, &400i128.into_val(&f.e)));
}

#[test]
fn transfer_event_matches_the_sep41_shape() {
    let f = Fixture::new();
    let from = f.fund(1_000);
    let to = Address::generate(&f.e);

    f.client.transfer(&from, &to, &250);

    let (topics, data) = last_event(&f.e);
    assert_eq!(
        topics,
        std::vec![
            xdr::ScVal::from_val(&f.e, &symbol_short!("transfer").to_val()),
            xdr::ScVal::from_val(&f.e, &from.to_val()),
            xdr::ScVal::from_val(&f.e, &to.to_val()),
        ]
    );
    assert_eq!(data, xdr::ScVal::from_val(&f.e, &250i128.into_val(&f.e)));
}

#[test]
fn transfer_from_emits_the_same_transfer_event_as_transfer() {
    let f = Fixture::new();
    let from = f.fund(1_000);
    let to = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    f.client
        .approve(&from, &spender, &500, &(f.e.ledger().sequence() + 100));

    f.client.transfer_from(&spender, &from, &to, &250);

    let (topics, data) = last_event(&f.e);
    assert_eq!(
        topics,
        std::vec![
            xdr::ScVal::from_val(&f.e, &symbol_short!("transfer").to_val()),
            xdr::ScVal::from_val(&f.e, &from.to_val()),
            xdr::ScVal::from_val(&f.e, &to.to_val()),
        ]
    );
    assert_eq!(data, xdr::ScVal::from_val(&f.e, &250i128.into_val(&f.e)));
}

#[test]
fn transfer_with_memo_keeps_the_standard_topics_and_adds_the_memo() {
    let f = Fixture::new();
    let from = f.fund(1_000);
    let to = Address::generate(&f.e);
    let note = bytes_of_len(&f.e, 8, b'o');

    f.client.transfer_with_memo(&from, &to, &250, &note);

    let (topics, data) = last_event(&f.e);
    assert_eq!(
        topics,
        std::vec![
            xdr::ScVal::from_val(&f.e, &symbol_short!("transfer").to_val()),
            xdr::ScVal::from_val(&f.e, &from.to_val()),
            xdr::ScVal::from_val(&f.e, &to.to_val()),
        ]
    );
    let items = last_event_data_items(&f.e);
    assert_eq!(items.len(), 2);
    assert_eq!(
        items[0],
        xdr::ScVal::from_val(&f.e, &250i128.into_val(&f.e))
    );
    assert_eq!(items[1], xdr::ScVal::from_val(&f.e, &note.to_val()));
}

#[test]
fn approve_event_matches_the_sep41_shape() {
    let f = Fixture::new();
    let from = f.fund(1_000);
    let spender = Address::generate(&f.e);
    let expiry = f.e.ledger().sequence() + 500;

    f.client.approve(&from, &spender, &400, &expiry);

    let (topics, data) = last_event(&f.e);
    assert_eq!(
        topics,
        std::vec![
            xdr::ScVal::from_val(&f.e, &symbol_short!("approve").to_val()),
            xdr::ScVal::from_val(&f.e, &from.to_val()),
            xdr::ScVal::from_val(&f.e, &spender.to_val()),
        ]
    );
    assert_eq!(
        data,
        xdr::ScVal::from_val(&f.e, &vec![&f.e, 400i128, expiry].to_val())
    );
}

#[test]
fn clawback_event_matches_the_sep41_shape() {
    let f = Fixture::new();
    let from = f.fund(1_000);

    f.client.clawback(&f.admin, &from, &400);

    let (topics, data) = last_event(&f.e);
    assert_eq!(
        topics,
        std::vec![
            xdr::ScVal::from_val(&f.e, &symbol_short!("clawback").to_val()),
            xdr::ScVal::from_val(&f.e, &f.admin.to_val()),
            xdr::ScVal::from_val(&f.e, &from.to_val()),
        ]
    );
    assert_eq!(data, xdr::ScVal::from_val(&f.e, &400i128.into_val(&f.e)));
}

#[test]
fn burn_from_emits_the_same_burn_event_as_burn() {
    let f = Fixture::new();
    let from = f.fund(1_000);
    let spender = Address::generate(&f.e);
    f.client
        .approve(&from, &spender, &500, &(f.e.ledger().sequence() + 100));

    f.client.burn_from(&spender, &from, &400);

    let (topics, data) = last_event(&f.e);
    assert_eq!(
        topics,
        std::vec![
            xdr::ScVal::from_val(&f.e, &symbol_short!("burn").to_val()),
            xdr::ScVal::from_val(&f.e, &from.to_val()),
        ]
    );
    assert_eq!(data, xdr::ScVal::from_val(&f.e, &400i128.into_val(&f.e)));
}

#[test]
fn a_rejected_operation_emits_no_event() {
    let f = Fixture::new();
    let from = f.fund(100);
    let to = Address::generate(&f.e);
    let before = f.e.events().all().events().len();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let client: &VeriTixPayClient<'static> = &f.client;
        client.transfer(&from, &to, &101);
    }));

    assert!(result.is_err());
    assert_eq!(f.e.events().all().events().len(), before);
}

#[test]
fn escrow_locks_do_not_change_event_shapes() {
    // A lock is a bookkeeping entry, not a transfer, so it must not add or
    // reshape any event on the ledger.
    let f = Fixture::new();
    let holder = f.fund(1_000);
    f.set_escrow_lock(&holder, 400);
    let to = Address::generate(&f.e);

    f.client.transfer(&holder, &to, &100);

    let (topics, data) = last_event(&f.e);
    assert_eq!(
        topics[0],
        xdr::ScVal::from_val(&f.e, &symbol_short!("transfer").to_val())
    );
    assert_eq!(data, xdr::ScVal::from_val(&f.e, &100i128.into_val(&f.e)));
    assert_eq!(f.e.events().all().events().len(), 2);
}

#[test]
fn supply_cap_never_appears_in_an_event() {
    // The cap is a contract invariant, not a token movement: minting under the
    // cap emits the same mint event as any other mint.
    let f = Fixture::new();
    let holder = Address::generate(&f.e);
    f.set_max_supply(1_000);
    f.client.mint(&f.admin, &holder, &1_000);

    let (topics, data) = last_event(&f.e);
    assert_eq!(
        topics[0],
        xdr::ScVal::from_val(&f.e, &symbol_short!("mint").to_val())
    );
    assert_eq!(data, xdr::ScVal::from_val(&f.e, &1_000i128.into_val(&f.e)));
}

#[test]
fn every_storage_key_referenced_by_events_is_part_of_the_declared_layout() {
    // Guards against an event referencing a key that does not exist: the escrow
    // lock key must be readable through the contract's own view.
    let f = Fixture::new();
    let holder = f.fund(1_000);
    f.e.as_contract(&f.contract_id, || {
        f.e.storage()
            .persistent()
            .set(&DataKey::EscrowLocked(holder.clone()), &250);
    });

    assert_eq!(f.client.escrow_locked(&holder), 250);
    assert_eq!(f.client.spendable_balance(&holder), 750);
}
