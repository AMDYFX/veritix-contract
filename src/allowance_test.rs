//! The two allowance rules most likely to regress.
//!
//! `approve` writes a grant with an expiration, `transfer_from` draws it down,
//! and the two are only correct together if both hold at once: a grant that
//! outlives its deadline is a live authorization nobody intended, and a spend
//! that does not reduce the grant by exactly the amount moved turns a
//! limited-power approval into a blank cheque. Both are easy to break with an
//! innocuous refactor — reordering a check, hoisting a read, replacing a
//! subtraction with an assignment — and neither shows up in the happy-path
//! tests that cover the rest of the token.
//!
//! Every test here pins one of those two rules, including the cases that are
//! easy to get wrong: spending the last unit of a grant, spending a grant to
//! exactly its remaining amount, spending nothing, spending against a grant
//! that lapsed between approval and use, and spending after the grant was
//! reduced to zero.

use crate::contract::testing::Fixture;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::Address;

/// Runs `f` and reports whether it panicked.
fn panics<R>(f: impl FnOnce() -> R) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err()
}

// ---- Expiry -------------------------------------------------------------

#[test]
fn an_expired_allowance_cannot_be_spent() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let receiver = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &500, &1_000);

    // The grant is live up to and including its expiration ledger.
    f.e.ledger().set_sequence_number(1_000);
    f.client.transfer_from(&spender, &holder, &receiver, &500);

    // Past it, the same spend is refused outright.
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let receiver = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &500, &1_000);
    f.e.ledger().set_sequence_number(1_001);

    assert!(panics(|| {
        f.client.transfer_from(&spender, &holder, &receiver, &100);
    }));
    assert_eq!(f.client.balance(&holder), 1_000);
    assert_eq!(f.client.balance(&receiver), 0);
}

#[test]
fn an_expiry_equal_to_the_current_ledger_is_still_live() {
    // `expiration_ledger` names the last ledger the grant is valid on, so a
    // spend on that exact ledger must succeed. Treating the deadline as
    // exclusive would expire every grant a ledger early.
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let receiver = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &500, &1_000);
    f.e.ledger().set_sequence_number(1_000);

    f.client.transfer_from(&spender, &holder, &receiver, &500);

    assert_eq!(f.client.balance(&receiver), 500);
}

#[test]
fn an_expiry_advances_the_ledger_and_expires_the_grant() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &500, &1_000);
    f.e.ledger().set_sequence_number(2_000);

    assert_eq!(f.client.allowance(&holder, &spender), 0);
}

#[test]
fn a_partial_spend_does_not_extend_the_deadline() {
    // The stored expiration is written back unchanged on every draw-down, so a
    // spend that happens right before the deadline does not silently push it
    // out by the same amount.
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let receiver = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &500, &1_000);

    f.client.transfer_from(&spender, &holder, &receiver, &100);

    assert_eq!(f.client.allowance_expiration(&holder, &spender), 1_000);
    assert_eq!(f.client.allowance(&holder, &spender), 400);
}

#[test]
fn an_expired_allowance_still_reports_its_expiration() {
    // The amount alone cannot tell an expired grant from a grant that never
    // existed; the expiration view is what distinguishes them.
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &500, &1_000);
    f.e.ledger().set_sequence_number(1_001);

    assert_eq!(f.client.allowance(&holder, &spender), 0);
    assert_eq!(f.client.allowance_expiration(&holder, &spender), 1_000);
}

#[test]
fn a_grant_never_made_reports_no_expiration() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let spender = Address::generate(&f.e);

    assert_eq!(f.client.allowance(&holder, &spender), 0);
    assert_eq!(f.client.allowance_expiration(&holder, &spender), 0);
}

#[test]
fn an_approval_past_in_the_future_is_refused() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let spender = Address::generate(&f.e);
    f.e.ledger().set_sequence_number(1_000);

    assert!(panics(|| {
        f.client.approve(&holder, &spender, &500, &999);
    }));
    assert_eq!(f.client.allowance(&holder, &spender), 0);
}

#[test]
fn a_regrant_replaces_the_expiration() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &500, &1_000);

    f.client.approve(&holder, &spender, &500, &5_000);

    assert_eq!(f.client.allowance_expiration(&holder, &spender), 5_000);
    assert_eq!(f.client.allowance(&holder, &spender), 500);
}

// ---- Exact decrement ----------------------------------------------------

#[test]
fn a_spend_reduces_the_allowance_by_exactly_the_amount_moved() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let receiver = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &500, &1_000_000);

    f.client.transfer_from(&spender, &holder, &receiver, &200);

    assert_eq!(f.client.allowance(&holder, &spender), 300);
    assert_eq!(f.client.balance(&holder), 800);
    assert_eq!(f.client.balance(&receiver), 200);
}

#[test]
fn repeated_spends_reduce_the_allowance_by_the_sum_of_the_amounts() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let a = Address::generate(&f.e);
    let b = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &500, &1_000_000);

    f.client.transfer_from(&spender, &holder, &a, &120);
    f.client.transfer_from(&spender, &holder, &b, &80);

    assert_eq!(f.client.allowance(&holder, &spender), 300);
    assert_eq!(f.client.balance(&a), 120);
    assert_eq!(f.client.balance(&b), 80);
}

#[test]
fn spending_the_remaining_allowance_exactly_exhausts_it() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let receiver = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &500, &1_000_000);

    f.client.transfer_from(&spender, &holder, &receiver, &200);
    f.client.transfer_from(&spender, &holder, &receiver, &300);

    assert_eq!(f.client.allowance(&holder, &spender), 0);
    assert_eq!(f.client.balance(&receiver), 500);
}

#[test]
fn one_token_more_than_the_allowance_is_refused() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let receiver = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &500, &1_000_000);

    assert!(panics(|| {
        f.client.transfer_from(&spender, &holder, &receiver, &501);
    }));
    // The refusal costs the spender nothing: the whole invocation reverted, so
    // the allowance is untouched.
    assert_eq!(f.client.allowance(&holder, &spender), 500);
    assert_eq!(f.client.balance(&holder), 1_000);
    assert_eq!(f.client.balance(&receiver), 0);
}

#[test]
fn a_spend_larger_than_the_balance_is_refused_without_touching_the_allowance() {
    let f = Fixture::new();
    let holder = f.fund(300);
    let receiver = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &500, &1_000_000);

    assert!(panics(|| {
        f.client.transfer_from(&spender, &holder, &receiver, &400);
    }));
    assert_eq!(f.client.allowance(&holder, &spender), 500);
    assert_eq!(f.client.balance(&holder), 300);
}

#[test]
fn spending_with_no_allowance_at_all_is_refused() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let receiver = Address::generate(&f.e);
    let spender = Address::generate(&f.e);

    assert!(panics(|| {
        f.client.transfer_from(&spender, &holder, &receiver, &1);
    }));
    assert_eq!(f.client.balance(&holder), 1_000);
}

#[test]
fn an_allowance_cannot_be_spent_twice() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let receiver = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &500, &1_000_000);

    f.client.transfer_from(&spender, &holder, &receiver, &500);
    assert!(panics(|| {
        f.client.transfer_from(&spender, &holder, &receiver, &1);
    }));

    assert_eq!(f.client.balance(&receiver), 500);
}

#[test]
fn one_token_is_the_smallest_positive_spend() {
    // Every amount check has to admit 1, or a grant can never be wound down to
    // exactly nothing by spending.
    let f = Fixture::new();
    let holder = f.fund(10);
    let receiver = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &10, &1_000_000);

    for _ in 0..10 {
        f.client.transfer_from(&spender, &holder, &receiver, &1);
    }

    assert_eq!(f.client.allowance(&holder, &spender), 0);
    assert_eq!(f.client.balance(&receiver), 10);
}

#[test]
fn a_zero_spend_is_refused_and_consumes_nothing() {
    // `consume_allowance` validates before it subtracts, so a zero cannot slip
    // through as a no-op that still touches storage.
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let receiver = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &500, &1_000_000);

    assert!(panics(|| {
        f.client.transfer_from(&spender, &holder, &receiver, &0);
    }));
    assert_eq!(f.client.allowance(&holder, &spender), 500);
}

#[test]
fn a_negative_spend_cannot_increase_the_allowance() {
    // The failure this guards against is a quiet one: subtracting a negative
    // amount would *raise* the grant, turning a limited approval into a larger
    // one without any successful transfer.
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let receiver = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &500, &1_000_000);

    assert!(panics(|| {
        f.client.transfer_from(&spender, &holder, &receiver, &-100);
    }));
    assert_eq!(f.client.allowance(&holder, &spender), 500);
    assert_eq!(f.client.balance(&holder), 1_000);
    assert_eq!(f.client.balance(&receiver), 0);
}

#[test]
fn the_allowance_is_reduced_by_exactly_the_amount_and_not_the_balance() {
    // A common implementation slips here: debiting the spender's balance
    // instead of the requested amount. The two only differ when the spender
    // holds tokens of their own.
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let spender = f.fund(1_000);
    let receiver = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &500, &1_000_000);

    f.client.transfer_from(&spender, &holder, &receiver, &200);

    assert_eq!(f.client.allowance(&holder, &spender), 300);
    assert_eq!(f.client.balance(&spender), 1_000);
}

#[test]
fn the_decrement_is_per_owner_and_spender_pair() {
    let f = Fixture::new();
    let one = f.fund(1_000);
    let two = f.fund(1_000);
    let receiver = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    f.client.approve(&one, &spender, &500, &1_000_000);
    f.client.approve(&two, &spender, &500, &1_000_000);

    f.client.transfer_from(&spender, &one, &receiver, &200);

    assert_eq!(f.client.allowance(&one, &spender), 300);
    assert_eq!(f.client.allowance(&two, &spender), 500);
}

// ---- increase_allowance / decrease_allowance ------------------------------

#[test]
fn increase_allowance_tops_up_a_live_grant_and_keeps_its_expiry() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &500, &1_000_000);

    // The second expiry is deliberately different: an increase must not
    // silently extend or shorten the window the owner chose.
    f.client
        .increase_allowance(&holder, &spender, &200, &2_000_000);

    assert_eq!(f.client.allowance(&holder, &spender), 700);
    assert_eq!(f.client.allowance_expiration(&holder, &spender), 1_000_000);
}

#[test]
fn increase_allowance_creates_a_grant_when_none_is_live() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let spender = Address::generate(&f.e);

    // No approval first: this call is the whole grant.
    f.client
        .increase_allowance(&holder, &spender, &300, &1_000_000);

    assert_eq!(f.client.allowance(&holder, &spender), 300);
    assert_eq!(f.client.allowance_expiration(&holder, &spender), 1_000_000);
}

#[test]
fn increase_allowance_ignores_an_expired_grant_and_starts_a_new_one() {
    // An expired grant must not be revived by topping it up; the increase
    // starts from the supplied expiry instead.
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &500, &10);

    f.e.ledger().set_sequence_number(11);
    f.client
        .increase_allowance(&holder, &spender, &200, &1_000_000);

    assert_eq!(f.client.allowance(&holder, &spender), 200);
    assert_eq!(f.client.allowance_expiration(&holder, &spender), 1_000_000);
}

#[test]
fn increase_allowance_refuses_a_negative_amount() {
    // A negative increase is a decrease; letting it through here would make the
    // two helpers interchangeable and the expiry rules ambiguous.
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &500, &1_000_000);

    assert!(panics(|| {
        f.client
            .increase_allowance(&holder, &spender, &-1, &1_000_000);
    }));
    assert_eq!(f.client.allowance(&holder, &spender), 500);
}

#[test]
fn increase_allowance_refuses_a_past_expiry_when_creating_a_grant() {
    // `approve` already refuses this for a fresh grant; the delta helper must
    // not become a way around that check.
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let spender = Address::generate(&f.e);

    f.e.ledger().set_sequence_number(100);
    assert!(panics(|| {
        f.client.increase_allowance(&holder, &spender, &100, &50);
    }));
}

#[test]
fn decrease_allowance_lowers_the_grant_and_keeps_its_expiry() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &500, &1_000_000);

    f.client.decrease_allowance(&holder, &spender, &200);

    assert_eq!(f.client.allowance(&holder, &spender), 300);
    assert_eq!(f.client.allowance_expiration(&holder, &spender), 1_000_000);
}

#[test]
fn decrease_allowance_saturates_at_zero_instead_of_underflowing() {
    // The failure this guards against is a wrap: 100 - 500 in i128 is a large
    // negative, and a negative allowance would read as *more* permission than
    // the owner granted rather than none at all.
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let receiver = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &100, &1_000_000);

    f.client.decrease_allowance(&holder, &spender, &500);

    assert_eq!(f.client.allowance(&holder, &spender), 0);

    // A grant reduced to zero is revoked, so it can no longer be spent.
    assert!(panics(|| {
        f.client.transfer_from(&spender, &holder, &receiver, &1);
    }));
    assert_eq!(f.client.balance(&holder), 1_000);
}

#[test]
fn decrease_allowance_refuses_a_negative_amount() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &500, &1_000_000);

    assert!(panics(|| {
        f.client.decrease_allowance(&holder, &spender, &-50);
    }));
    assert_eq!(f.client.allowance(&holder, &spender), 500);
}

#[test]
fn allowance_expiration_reports_the_stored_expiry_and_zero_when_unset() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let receiver = Address::generate(&f.e);
    let spender = Address::generate(&f.e);

    assert_eq!(f.client.allowance_expiration(&holder, &spender), 0);

    f.client.approve(&holder, &spender, &500, &4_321);
    assert_eq!(f.client.allowance_expiration(&holder, &spender), 4_321);

    // A spend reduces the amount, never the deadline.
    f.client.transfer_from(&spender, &holder, &receiver, &100);
    assert_eq!(f.client.allowance_expiration(&holder, &spender), 4_321);
}

// ---- transfer_from -------------------------------------------------------

#[test]
fn transfer_from_spends_the_allowance_before_moving_the_tokens() {
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let receiver = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &300, &1_000_000);

    f.client.transfer_from(&spender, &holder, &receiver, &300);

    assert_eq!(f.client.allowance(&holder, &spender), 0);
    assert_eq!(f.client.balance(&holder), 700);
    assert_eq!(f.client.balance(&receiver), 300);
}

#[test]
fn transfer_from_moves_nothing_when_the_allowance_is_insufficient() {
    // The allowance is consumed before the transfer, so an over-large spend
    // aborts before the recipient is paid.
    let f = Fixture::new();
    let holder = f.fund(1_000);
    let receiver = Address::generate(&f.e);
    let spender = Address::generate(&f.e);
    f.client.approve(&holder, &spender, &100, &1_000_000);

    assert!(panics(|| {
        f.client.transfer_from(&spender, &holder, &receiver, &101);
    }));
    assert_eq!(f.client.balance(&receiver), 0);
    assert_eq!(f.client.balance(&holder), 1_000);
}
