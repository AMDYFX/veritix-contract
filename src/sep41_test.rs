//! SEP-41 conformance suite.
//!
//! The token advertises SEP-41 compliance, and this module is what makes that
//! claim falsifiable: every function the standard requires is exercised here
//! through the public client, so a change that quietly breaks the interface, the
//! allowance expiration semantics, or an event shape fails a test rather than
//! failing silently in someone else's wallet.
//!
//! The companion document is `docs/sep41-compliance.md`.

use crate::contract::testing::{bytes_of_len, no_holders, Fixture};
use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
use soroban_sdk::{symbol_short, vec, xdr, Address, FromVal, String};

/// Reads the topics of the most recent event.
fn last_topics(e: &soroban_sdk::Env) -> std::vec::Vec<xdr::ScVal> {
    let events = e.events().all();
    let last = events.events().last().expect("no event was emitted");
    let xdr::ContractEventBody::V0(body) = &last.body else {
        panic!("expected a v0 contract event");
    };
    body.topics.clone()
}

// ---- Metadata ------------------------------------------------------------

#[test]
fn name_symbol_and_decimals_are_reported() {
    let f = Fixture::new();
    assert_eq!(f.client.name(), String::from_str(&f.e, "VeriTix"));
    assert_eq!(f.client.symbol(), String::from_str(&f.e, "VTX"));
    assert_eq!(f.client.decimals(), 7);
}

#[test]
fn metadata_can_be_supplied_at_initialization() {
    let e = soroban_sdk::Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, crate::contract::VeriTixPay);
    let client = crate::contract::VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);

    client.initialize_with_metadata(
        &admin,
        &String::from_str(&e, "Event Tickets"),
        &String::from_str(&e, "VTIX"),
        &2,
    );

    assert_eq!(client.name(), String::from_str(&e, "Event Tickets"));
    assert_eq!(client.symbol(), String::from_str(&e, "VTIX"));
    assert_eq!(client.decimals(), 2);
}

// ---- Balances and supply -------------------------------------------------

#[test]
fn total_supply_is_zero_before_any_mint() {
    let f = Fixture::new();
    assert_eq!(f.client.total_supply(), 0);
}

#[test]
fn balance_of_an_unknown_account_is_zero() {
    let f = Fixture::new();
    let stranger = Address::generate(&f.e);
    assert_eq!(f.client.balance(&stranger), 0);
}

#[test]
fn total_supply_tracks_mint_and_burn() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    assert_eq!(f.client.total_supply(), 1_000);

    f.client.burn(&holder, &400);
    assert_eq!(f.client.total_supply(), 600);
}

#[test]
fn transferring_does_not_change_total_supply() {
    let f = Fixture::new();
    let from = f.fund(1_000);
    let to = Address::generate(&f.e);

    f.client.transfer(&from, &to, &1_000);

    assert_eq!(f.client.total_supply(), 1_000);
    assert_eq!(f.client.balance(&from), 0);
    assert_eq!(f.client.balance(&to), 1_000);
}

#[test]
fn spendable_balance_equals_the_balance_when_nothing_is_locked() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    assert_eq!(f.client.spendable_balance(&holder), 1_000);
}

#[test]
fn spendable_balance_excludes_escrow_locks() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    f.set_escrow_lock(&holder, 400);

    assert_eq!(f.client.balance(&holder), 1_000);
    assert_eq!(f.client.spendable_balance(&holder), 600);
}

#[test]
fn spendable_balance_is_zero_for_a_frozen_account() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    f.client.set_frozen(&f.admin, &holder, &true);

    assert_eq!(f.client.balance(&holder), 1_000);
    assert_eq!(f.client.spendable_balance(&holder), 0);
}

#[test]
fn total_holders_and_get_holders_report_the_holder_set() {
    let f = Fixture::new();
    assert_eq!(f.client.total_holders(), 0);
    assert_eq!(f.client.get_holders(), no_holders(&f.e));

    let alice = f.fund(1_000);
    let bob = f.fund(500);
    assert_eq!(f.client.total_holders(), 2);
    assert_eq!(
        f.client.get_holders(),
        vec![&f.e, alice.clone(), bob.clone()]
    );

    f.client.burn(&alice, &1_000);
    assert_eq!(f.client.total_holders(), 1);
    assert_eq!(f.client.get_holders(), vec![&f.e, bob.clone()]);
    assert_eq!(
        f.client.get_holders().len(),
        usize::try_from(f.client.total_holders()).unwrap()
    );
}

// ---- Allowances ----------------------------------------------------------

#[test]
fn allowance_is_zero_before_any_approval() {
    let f = Fixture::new();
    let from = f.fund(1_000);
    let spender = Address::generate(&f.e);
    assert_eq!(f.client.allowance(&from, &spender), 0);
}

#[test]
fn approve_sets_the_allowance_and_reports_its_expiration() {
    let f = Fixture::new();
    let from = f.fund(1_000);
    let spender = Address::generate(&f.e);
    let expiry = f.e.ledger().sequence() + 1_000;

    f.client.approve(&from, &spender, &500, &expiry);

    assert_eq!(f.client.allowance(&from, &spender), 500);
    assert_eq!(f.client.allowance_expiration(&from, &spender), expiry);
}

#[test]
fn approve_overwrites_rather_than_accumulates() {
    let f = Fixture::new();
    let from = f.fund(1_000);
    let spender = Address::generate(&f.e);
    let expiry = f.e.ledger().sequence() + 1_000;

    f.client.approve(&from, &spender, &500, &expiry);
    f.client.approve(&from, &spender, &200, &expiry);

    assert_eq!(f.client.allowance(&from, &spender), 200);
}

#[test]
fn approving_zero_revokes_the_allowance() {
    let f = Fixture::new();
    let from = f.fund(1_000);
    let spender = Address::generate(&f.e);
    let expiry = f.e.ledger().sequence() + 1_000;

    f.client.approve(&from, &spender, &500, &expiry);
    f.client.approve(&from, &spender, &0, &0);

    assert_eq!(f.client.allowance(&from, &spender), 0);
    assert_eq!(f.client.allowance_expiration(&from, &spender), 0);
}

#[test]
fn approving_a_past_expiration_is_rejected_for_a_non_zero_amount() {
    let f = Fixture::new();
    let from = f.fund(1_000);
    let spender = Address::generate(&f.e);
    f.e.ledger().set_sequence_number(1_000);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.approve(&from, &spender, &500, &999);
    }));

    assert!(result.is_err());
    assert_eq!(f.client.allowance(&from, &spender), 0);
}

#[test]
fn approving_zero_with_a_past_expiration_is_allowed() {
    // Revoking an allowance has no expiry to get wrong, so a zero amount is
    // accepted regardless of the ledger requested.
    let f = Fixture::new();
    let from = f.fund(1_000);
    let spender = Address::generate(&f.e);
    f.e.ledger().set_sequence_number(1_000);

    f.client.approve(&from, &spender, &0, &0);

    assert_eq!(f.client.allowance(&from, &spender), 0);
}

#[test]
fn an_allowance_expires_once_its_ledger_is_behind() {
    let f = Fixture::new();
    let from = f.fund(1_000);
    let spender = Address::generate(&f.e);
    f.client.approve(&from, &spender, &500, &1_000);

    f.e.ledger().set_sequence_number(1_000);
    assert_eq!(f.client.allowance(&from, &spender), 500);

    f.e.ledger().set_sequence_number(1_001);
    assert_eq!(f.client.allowance(&from, &spender), 0);
}

#[test]
fn an_expired_allowance_cannot_be_spent() {
    let f = Fixture::new();
    let from = f.fund(1_000);
    let to = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    f.client.approve(&from, &spender, &500, &1_000);
    f.e.ledger().set_sequence_number(1_001);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.transfer_from(&spender, &from, &to, &100);
    }));

    assert!(result.is_err());
    assert_eq!(f.client.balance(&from), 1_000);
    assert_eq!(f.client.balance(&to), 0);
}

// ---- transfer_from -------------------------------------------------------

#[test]
fn transfer_from_spends_the_allowance() {
    let f = Fixture::new();
    let from = f.fund(1_000);
    let to = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    f.client
        .approve(&from, &spender, &500, &(f.e.ledger().sequence() + 1_000));

    f.client.transfer_from(&spender, &from, &to, &200);

    assert_eq!(f.client.balance(&from), 800);
    assert_eq!(f.client.balance(&to), 200);
    assert_eq!(f.client.allowance(&from, &spender), 300);
}

#[test]
fn transfer_from_cannot_exceed_the_allowance() {
    let f = Fixture::new();
    let from = f.fund(1_000);
    let to = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    f.client
        .approve(&from, &spender, &500, &(f.e.ledger().sequence() + 1_000));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.transfer_from(&spender, &from, &to, &501);
    }));

    assert!(result.is_err());
    assert_eq!(f.client.balance(&from), 1_000);
    assert_eq!(f.client.allowance(&from, &spender), 500);
}

#[test]
fn transfer_from_is_bounded_by_the_balance_too() {
    let f = Fixture::new();
    let from = f.fund(100);
    let to = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    f.client
        .approve(&from, &spender, &500, &(f.e.ledger().sequence() + 1_000));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.transfer_from(&spender, &from, &to, &101);
    }));

    assert!(result.is_err());
    assert_eq!(f.client.balance(&from), 100);
    assert_eq!(f.client.allowance(&from, &spender), 500);
}

#[test]
fn an_exhausted_allowance_is_revoked() {
    let f = Fixture::new();
    let from = f.fund(1_000);
    let to = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    f.client
        .approve(&from, &spender, &200, &(f.e.ledger().sequence() + 1_000));

    f.client.transfer_from(&spender, &from, &to, &200);

    assert_eq!(f.client.allowance(&from, &spender), 0);
    assert_eq!(f.client.allowance_expiration(&from, &spender), 0);
}

// ---- burn_from -----------------------------------------------------------

#[test]
fn burn_from_spends_the_allowance_and_reduces_supply() {
    let f = Fixture::new();
    let from = f.fund(1_000);
    let spender = Address::generate(&f.e);
    f.client
        .approve(&from, &spender, &500, &(f.e.ledger().sequence() + 1_000));

    f.client.burn_from(&spender, &from, &400);

    assert_eq!(f.client.balance(&from), 600);
    assert_eq!(f.client.total_supply(), 600);
    assert_eq!(f.client.allowance(&from, &spender), 100);
}

#[test]
fn burn_from_cannot_exceed_the_allowance() {
    let f = Fixture::new();
    let from = f.fund(1_000);
    let spender = Address::generate(&f.e);
    f.client
        .approve(&from, &spender, &500, &(f.e.ledger().sequence() + 1_000));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.burn_from(&spender, &from, &501);
    }));

    assert!(result.is_err());
    assert_eq!(f.client.balance(&from), 1_000);
    assert_eq!(f.client.total_supply(), 1_000);
}

// ---- clawback ------------------------------------------------------------

#[test]
fn clawback_reduces_the_balance_and_supply() {
    let f = Fixture::new();
    let from = f.fund(1_000);

    f.client.clawback(&f.admin, &from, &400);

    assert_eq!(f.client.balance(&from), 600);
    assert_eq!(f.client.total_supply(), 600);
}

#[test]
fn clawback_is_admin_only() {
    let f = Fixture::new();
    let from = f.fund(1_000);
    let stranger = Address::generate(&f.e);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.clawback(&stranger, &from, &400);
    }));

    assert!(result.is_err());
    assert_eq!(f.client.balance(&from), 1_000);
    assert_eq!(f.client.total_supply(), 1_000);
}

// ---- Rejections ----------------------------------------------------------

#[test]
fn non_positive_amounts_are_rejected_everywhere() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let other = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    let expiry = f.e.ledger().sequence() + 1_000;
    f.client.approve(&holder, &spender, &500, &expiry);

    for amount in [0i128, -1] {
        let client = &f.client;
        let holder = &holder;
        let other = &other;
        let spender = &spender;
        let expiry = &expiry;
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.transfer(holder, other, &amount);
        }))
        .is_err());
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.transfer_from(spender, holder, other, &amount);
        }))
        .is_err());
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.burn(holder, &amount);
        }))
        .is_err());
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.burn_from(spender, holder, &amount);
        }))
        .is_err());
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.transfer_with_memo(holder, other, &amount, &bytes_of_len(&f.e, 4, b'x'));
        }))
        .is_err());
    }
}

#[test]
fn an_oversized_memo_is_rejected() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let to = Address::generate(&f.e);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client
            .transfer_with_memo(&holder, &to, &100, &bytes_of_len(&f.e, 65, b'x'));
    }));

    assert!(result.is_err());
    assert_eq!(f.client.balance(&holder), 1_000);
    assert_eq!(f.client.balance(&to), 0);
}

#[test]
fn a_memo_at_the_limit_is_accepted() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let to = Address::generate(&f.e);

    f.client
        .transfer_with_memo(&holder, &to, &100, &bytes_of_len(&f.e, 64, b'x'));

    assert_eq!(f.client.balance(&to), 100);
}

// ---- Event shapes --------------------------------------------------------

#[test]
fn every_required_event_uses_the_standard_topic() {
    let f = Fixture::new();
    let from = f.fund(1_000);
    let to = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    let expiry = f.e.ledger().sequence() + 1_000;

    f.client.approve(&from, &spender, &500, &expiry);
    assert_eq!(
        last_topics(&f.e)[0],
        xdr::ScVal::from_val(&f.e, &symbol_short!("approve").to_val())
    );

    f.client.transfer(&from, &to, &100);
    assert_eq!(
        last_topics(&f.e)[0],
        xdr::ScVal::from_val(&f.e, &symbol_short!("transfer").to_val())
    );

    f.client.transfer_from(&spender, &from, &to, &100);
    assert_eq!(
        last_topics(&f.e)[0],
        xdr::ScVal::from_val(&f.e, &symbol_short!("transfer").to_val())
    );

    f.client.burn(&from, &50);
    assert_eq!(
        last_topics(&f.e)[0],
        xdr::ScVal::from_val(&f.e, &symbol_short!("burn").to_val())
    );

    f.client.burn_from(&spender, &from, &50);
    assert_eq!(
        last_topics(&f.e)[0],
        xdr::ScVal::from_val(&f.e, &symbol_short!("burn").to_val())
    );

    f.client.clawback(&f.admin, &from, &50);
    assert_eq!(
        last_topics(&f.e)[0],
        xdr::ScVal::from_val(&f.e, &symbol_short!("clawback").to_val())
    );

    f.client.mint(&f.admin, &to, &50);
    assert_eq!(
        last_topics(&f.e)[0],
        xdr::ScVal::from_val(&f.e, &symbol_short!("mint").to_val())
    );
}

// ---- Composition ---------------------------------------------------------

#[test]
fn a_full_lifecycle_keeps_supply_and_balances_consistent() {
    let f = Fixture::new();
    let alice = f.fund(10_000);
    let bob = Address::generate(&f.e);
    let carol = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    let expiry = f.e.ledger().sequence() + 1_000;

    // alice -> bob direct
    f.client.transfer(&alice, &bob, &3_000);
    // bob -> carol by allowance
    f.client.approve(&bob, &spender, &2_000, &expiry);
    f.client.transfer_from(&spender, &bob, &carol, &2_000);
    // carol burns half
    f.client.burn(&carol, &1_000);
    // admin claws back the rest of bob's position
    f.client.clawback(&f.admin, &bob, &1_000);

    assert_eq!(f.client.balance(&alice), 7_000);
    assert_eq!(f.client.balance(&bob), 0);
    assert_eq!(f.client.balance(&carol), 1_000);
    assert_eq!(f.client.total_supply(), 8_000);
    assert_eq!(f.client.total_holders(), 2);
    assert_eq!(
        f.client.get_holders(),
        vec![&f.e, alice.clone(), carol.clone()]
    );
    assert_eq!(f.client.allowance(&bob, &spender), 0);
}

#[test]
fn a_frozen_account_cannot_move_tokens_in_either_direction() {
    let f = Fixture::new();
    let alice = f.fund(1_000);
    let bob = Address::generate(&f.e);
    f.client.set_frozen(&f.admin, &bob, &true);

    let outbound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.transfer(&bob, &alice, &1);
    }));
    assert!(outbound.is_err());

    f.client.transfer(&alice, &bob, &1);
    let inbound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.transfer(&alice, &bob, &1);
    }));
    assert!(inbound.is_err());

    assert_eq!(f.client.balance(&alice), 1_000);
    assert_eq!(f.client.balance(&bob), 1);
}

#[test]
fn a_paused_contract_moves_no_tokens() {
    let f = Fixture::new();
    let alice = f.fund(1_000);
    let bob = Address::generate(&f.e);
    f.client.set_paused(&f.admin, &true);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.transfer(&alice, &bob, &1);
    }));

    assert!(result.is_err());
    assert_eq!(f.client.balance(&bob), 0);
}

#[test]
fn the_holders_agree_with_the_holder_set() {
    let f = Fixture::new();
    let alice = f.fund(1_000);
    let bob = f.fund(1_000);
    let listed = f.client.get_holders();
    assert_eq!(listed.len(), 2);
    assert!(listed.contains(alice.clone()));
    assert!(listed.contains(bob));
}
