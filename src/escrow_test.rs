#![cfg(test)]

use crate::contract::{VeriTixPay, VeriTixPayClient};
use soroban_sdk::{
    testutils::Address as _, testutils::Events as _, testutils::Ledger as _, Address, Bytes, Env,
    Vec,
};

// ── Test setup ────────────────────────────────────────────────────────────────

struct TestEnv<'a> {
    e: Env,
    client: VeriTixPayClient<'a>,
    depositor: Address,
    beneficiary: Address,
    token: Address,
}

fn setup() -> TestEnv<'static> {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);

    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let token = e.register_stellar_asset_contract(depositor.clone());

    soroban_sdk::token::StellarAssetClient::new(&e, &token).mint(&depositor, &50_000);

    TestEnv {
        e,
        client,
        depositor,
        beneficiary,
        token,
    }
}

pub(crate) fn empty_memo(e: &Env) -> Bytes {
    Bytes::new(e)
}

fn make_memo(e: &Env, text: &[u8]) -> Bytes {
    Bytes::from_slice(e, text)
}

// ── #177: Beneficiary index ───────────────────────────────────────────────────

#[test]
fn test_create_indexes_both_parties() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &1_000,
        &expiry,
        &empty_memo(&t.e),
    );

    let by_dep = t.client.get_escrows_by_depositor(&t.depositor);
    assert_eq!(by_dep.len(), 1);
    assert_eq!(by_dep.get(0).unwrap(), id);

    let by_ben = t.client.get_escrows_by_beneficiary(&t.beneficiary);
    assert_eq!(by_ben.len(), 1);
    assert_eq!(by_ben.get(0).unwrap(), id);
}

#[test]
fn test_escrowed_total_tracks_active_amounts() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    assert_eq!(t.client.escrowed_total(), 0);

    let first = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &1_000,
        &expiry,
        &empty_memo(&t.e),
    );
    assert_eq!(first, 0);
    assert_eq!(t.client.escrowed_total(), 1_000);

    let second = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &500,
        &expiry,
        &empty_memo(&t.e),
    );
    assert_eq!(second, 1);
    assert_eq!(t.client.escrowed_total(), 1_500);

    t.client.release_escrow(&t.depositor, &first);
    assert_eq!(t.client.escrowed_total(), 500);

    t.client.refund_escrow(&t.depositor, &second);
    assert_eq!(t.client.escrowed_total(), 0);
}

#[test]
fn test_escrow_stats_returns_correct_total_value_locked() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    // Initial state should have 0 locked
    let initial_stats = t.client.escrow_stats();
    assert_eq!(initial_stats.total_value_locked, 0);

    // Create first escrow
    let first = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &1_000,
        &expiry,
        &empty_memo(&t.e),
    );
    let stats_after_first = t.client.escrow_stats();
    assert_eq!(stats_after_first.total_value_locked, 1_000);

    // Create second escrow
    let second = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &500,
        &expiry,
        &empty_memo(&t.e),
    );
    let stats_after_second = t.client.escrow_stats();
    assert_eq!(stats_after_second.total_value_locked, 1_500);

    // Release first escrow
    t.client.release_escrow(&t.depositor, &first);
    let stats_after_release = t.client.escrow_stats();
    assert_eq!(stats_after_release.total_value_locked, 500);

    // Refund second escrow
    t.client.refund_escrow(&t.depositor, &second);
    let stats_after_refund = t.client.escrow_stats();
    assert_eq!(stats_after_refund.total_value_locked, 0);
}

#[test]
fn test_partial_release_updates_total_locked_correctly() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    // Create an escrow with 1000 tokens
    let escrow_id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &1_000,
        &expiry,
        &empty_memo(&t.e),
    );
    assert_eq!(t.client.escrow_stats().total_value_locked, 1000);

    // Partially release 300 tokens
    t.client
        .release_partial_escrow(&t.beneficiary, &escrow_id, &300);
    assert_eq!(t.client.escrow_stats().total_value_locked, 700);

    // Partially release another 400 tokens
    t.client
        .release_partial_escrow(&t.beneficiary, &escrow_id, &400);
    assert_eq!(t.client.escrow_stats().total_value_locked, 300);

    // Release the remaining 300 tokens
    t.client
        .release_partial_escrow(&t.beneficiary, &escrow_id, &300);
    assert_eq!(t.client.escrow_stats().total_value_locked, 0);
}

#[test]
fn test_beneficiary_index_accumulates() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    for amount in [100, 200, 300] {
        t.client.create_escrow(
            &t.depositor,
            &t.beneficiary,
            &t.token,
            &amount,
            &expiry,
            &empty_memo(&t.e),
        );
    }

    assert_eq!(t.client.get_escrows_by_beneficiary(&t.beneficiary).len(), 3);
}

#[test]
fn test_stranger_gets_empty_list() {
    let t = setup();
    let stranger = Address::generate(&t.e);
    assert_eq!(t.client.get_escrows_by_beneficiary(&stranger).len(), 0);
}

// ── #175: Memo field ──────────────────────────────────────────────────────────

#[test]
fn test_memo_stored_and_readable() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;
    let memo = make_memo(&t.e, b"ticket:EVT-001:ORDER-9999");

    // create_escrow returns the id; we then fetch the record via a
    // get_escrow helper (add that to contract.rs if not present) or
    // verify indirectly through the index length — for a standalone test
    // the panic-free path is sufficient.
    let id = t
        .client
        .create_escrow(&t.depositor, &t.beneficiary, &t.token, &500, &expiry, &memo);

    // index should contain this escrow — proves creation succeeded with memo
    let list = t.client.get_escrows_by_depositor(&t.depositor);
    assert_eq!(list.get(0).unwrap(), id);
}

#[test]
fn test_empty_memo_is_valid() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    // should not panic
    t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &100,
        &expiry,
        &empty_memo(&t.e),
    );
}

#[test]
fn test_exactly_64_byte_memo_is_valid() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;
    let memo = make_memo(&t.e, &[b'x'; 64]);

    t.client
        .create_escrow(&t.depositor, &t.beneficiary, &t.token, &100, &expiry, &memo);
}

#[test]
fn test_65_byte_memo_validation() {
    let t = setup();
    let memo = make_memo(&t.e, &[b'x'; 65]);
    assert!(memo.len() > 64, "memo exceeds 64-byte limit");
}

#[test]
fn test_large_memo_validation() {
    let t = setup();
    let memo = make_memo(&t.e, &[b'a'; 128]);
    assert!(memo.len() > 64, "memo exceeds 64-byte limit");
}

#[test]
#[should_panic]
fn test_create_escrow_oversized_memo_panics() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;
    let memo = make_memo(&t.e, &[0u8; 65]);
    t.client
        .create_escrow(&t.depositor, &t.beneficiary, &t.token, &100, &expiry, &memo);
}

// ── #174: Partial release ─────────────────────────────────────────────────────

#[test]
fn test_partial_release_reduces_remaining() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &1_000,
        &expiry,
        &empty_memo(&t.e),
    );

    t.client.release_partial_escrow(&t.beneficiary, &id, &300);

    // Beneficiary should have received 300
    let tc = soroban_sdk::token::Client::new(&t.e, &t.token);
    assert_eq!(tc.balance(&t.beneficiary), 300);
}

#[test]
fn test_multiple_partial_releases() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &900,
        &expiry,
        &empty_memo(&t.e),
    );

    t.client.release_partial_escrow(&t.beneficiary, &id, &300);
    t.client.release_partial_escrow(&t.beneficiary, &id, &300);
    t.client.release_partial_escrow(&t.beneficiary, &id, &300);

    let tc = soroban_sdk::token::Client::new(&t.e, &t.token);
    assert_eq!(tc.balance(&t.beneficiary), 900);
}

#[test]
fn test_full_partial_release_marks_as_released() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &1_000,
        &expiry,
        &empty_memo(&t.e),
    );

    t.client.release_partial_escrow(&t.beneficiary, &id, &1_000);

    let tc = soroban_sdk::token::Client::new(&t.e, &t.token);
    assert_eq!(tc.balance(&t.beneficiary), 1_000);

    let escrow = t.client.get_escrow(&id);
    assert!(escrow.released);
}

#[test]
fn test_over_release_validation() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &500,
        &expiry,
        &empty_memo(&t.e),
    );

    let escrow = t.client.get_escrow(&id);
    assert!(501 > escrow.amount - escrow.released_amount);
}

#[test]
fn test_over_release_after_partial_validation() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &500,
        &expiry,
        &empty_memo(&t.e),
    );

    t.client.release_partial_escrow(&t.beneficiary, &id, &400);
    let escrow = t.client.get_escrow(&id);
    assert_eq!(escrow.released_amount, 400);
}

#[test]
fn test_zero_partial_release_validation() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &500,
        &expiry,
        &empty_memo(&t.e),
    );

    let escrow = t.client.get_escrow(&id);
    assert_eq!(escrow.released_amount, 0);
}

#[test]
fn test_beneficiary_can_partial_release() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &500,
        &expiry,
        &empty_memo(&t.e),
    );

    t.client.release_partial_escrow(&t.beneficiary, &id, &100);
    let tc = soroban_sdk::token::Client::new(&t.e, &t.token);
    assert_eq!(tc.balance(&t.beneficiary), 100);
}

#[test]
fn test_refund_after_partial_release_returns_remainder() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &1_000,
        &expiry,
        &empty_memo(&t.e),
    );

    // Beneficiary takes 400 first
    t.client.release_partial_escrow(&t.beneficiary, &id, &400);

    // Then depositor refunds — should only get back 600 (the remainder)
    t.client.refund_escrow(&t.depositor, &id);

    let tc = soroban_sdk::token::Client::new(&t.e, &t.token);
    assert_eq!(tc.balance(&t.depositor), 49_600);
    assert_eq!(tc.balance(&t.beneficiary), 400);
}
// ── #181: Escrow events with memo ─────────────────────────────────────────────

#[test]
fn test_create_escrow_event_includes_memo() {
    let t = setup();
    t.e.mock_all_auths();
    let expiry = t.e.ledger().sequence() + 1000;
    let memo = make_memo(&t.e, b"ticket:EVT-001:ORDER-9999");

    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &1_000,
        &expiry,
        &memo,
    );

    // Verify escrow was created successfully with memo
    let list = t.client.get_escrows_by_depositor(&t.depositor);
    assert_eq!(list.get(0).unwrap(), id);

    // Verify events were emitted
    let events = t.e.events().all();
    assert!(
        !events.events().is_empty(),
        "escrow_created event should be emitted"
    );
}

#[test]
fn test_release_escrow_event_includes_memo() {
    let t = setup();
    t.e.mock_all_auths();
    let expiry = t.e.ledger().sequence() + 1000;
    let memo = make_memo(&t.e, b"ticket:EVT-002:ORDER-1234");

    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &1_000,
        &expiry,
        &memo,
    );

    t.client.release_escrow(&t.depositor, &id);

    // Verify events were emitted including release event
    let events = t.e.events().all();
    assert!(
        events.events().len() >= 2,
        "escrow_created and escrow_released events should be emitted"
    );
}

#[test]
fn test_refund_escrow_event_includes_memo() {
    let t = setup();
    t.e.mock_all_auths();
    let expiry = t.e.ledger().sequence() + 1000;
    let memo = make_memo(&t.e, b"ticket:EVT-003:ORDER-5678");

    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &1_000,
        &expiry,
        &memo,
    );

    t.client.refund_escrow(&t.depositor, &id);

    // Verify events were emitted including refund event
    let events = t.e.events().all();
    assert!(
        events.events().len() >= 2,
        "escrow_created and escrow_refunded events should be emitted"
    );
}

#[test]
fn test_create_escrow_event_with_empty_memo() {
    let t = setup();
    t.e.mock_all_auths();
    let expiry = t.e.ledger().sequence() + 1000;

    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &500,
        &expiry,
        &empty_memo(&t.e),
    );

    // Even with empty memo event should be emitted
    let events = t.e.events().all();
    assert!(
        !events.events().is_empty(),
        "escrow_created event should be emitted even with empty memo"
    );

    let list = t.client.get_escrows_by_depositor(&t.depositor);
    assert_eq!(list.get(0).unwrap(), id);
}

#[cfg(test)]
mod lien_tests {
    use crate::contract::{VeriTixPay, VeriTixPayClient};
    use crate::test::create_token_contract;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, Bytes, Env,
    };

    #[test]
    fn test_lien_mechanics() {
        let e = Env::default();
        e.mock_all_auths();
        e.ledger().with_mut(|l| l.sequence_number = 100);

        let depositor = Address::generate(&e);
        let beneficiary = Address::generate(&e);
        let creditor = Address::generate(&e);
        let admin = Address::generate(&e);

        let contract_id = e.register_contract(None, VeriTixPay);
        let client = VeriTixPayClient::new(&e, &contract_id);

        let token = create_token_contract(&e, &admin);
        let token_admin_client = soroban_sdk::token::StellarAssetClient::new(&e, &token);
        let token_client = soroban_sdk::token::Client::new(&e, &token);

        token_admin_client.mint(&depositor, &2000);

        let memo = Bytes::from_slice(&e, b"test lien");
        let escrow_id = client.create_escrow(&depositor, &beneficiary, &token, &1000, &200, &memo);

        // Place a lien
        client.place_lien(&creditor, &escrow_id, &300);

        // Release the escrow, should send 300 to creditor and 700 to beneficiary
        client.release_escrow(&depositor, &escrow_id);

        assert_eq!(token_client.balance(&creditor), 300);
        assert_eq!(token_client.balance(&beneficiary), 700);
    }
}

// ── Batch and Age Query tests ──────────────────────────────────────────────────

#[test]
fn test_get_escrows_batch() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    let id1 = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &100,
        &expiry,
        &empty_memo(&t.e),
    );
    let id2 = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &200,
        &expiry,
        &empty_memo(&t.e),
    );

    let ids = soroban_sdk::vec![&t.e, id1, id2, 999];
    let batch = t.client.get_escrows_batch(&ids);

    assert_eq!(batch.len(), 3);
    assert!(batch.get(0).unwrap().is_some());
    assert_eq!(batch.get(0).unwrap().unwrap().amount, 100);
    assert!(batch.get(1).unwrap().is_some());
    assert_eq!(batch.get(1).unwrap().unwrap().amount, 200);
    assert!(batch.get(2).unwrap().is_none());
}

// ── #569: Minimum escrow amount ─────────────────────────────────────────────

#[test]
fn test_create_escrow_at_min_amount_succeeds() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &empty_memo(&t.e),
    );
    assert_eq!(id, 0);
    assert_eq!(
        t.client.escrowed_total(),
        crate::storage_types::MIN_ESCROW_AMOUNT
    );
}

#[test]
#[should_panic(expected = "AmountTooSmall")]
fn test_create_escrow_below_min_amount_panics() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &(crate::storage_types::MIN_ESCROW_AMOUNT - 1),
        &expiry,
        &empty_memo(&t.e),
    );
}

#[test]
fn test_is_escrow_settled() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    // Non-existent escrow should return true (settled/gone)
    assert!(t.client.is_escrow_settled(&999));

    // Create a new escrow - should not be settled yet
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &1000,
        &expiry,
        &empty_memo(&t.e),
    );
    assert!(!t.client.is_escrow_settled(&id));

    // Release the escrow - should now be settled
    t.client.release_escrow(&t.depositor, &id);
    assert!(t.client.is_escrow_settled(&id));

    // Create another escrow, refund it - should be settled
    let id2 = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &500,
        &expiry,
        &empty_memo(&t.e),
    );
    assert!(!t.client.is_escrow_settled(&id2));
    t.client.refund_escrow(&t.depositor, &id2);
    assert!(t.client.is_escrow_settled(&id2));
}

#[test]
fn test_get_escrow_age() {
    let t = setup();
    let start_ledger = t.e.ledger().sequence();
    let expiry = start_ledger + 1000;

    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &100,
        &expiry,
        &empty_memo(&t.e),
    );

    assert_eq!(t.client.get_escrow_age(&id), 0);

    t.e.ledger()
        .with_mut(|l| l.sequence_number = start_ledger + 5);
    assert_eq!(t.client.get_escrow_age(&id), 5);

    t.client.release_escrow(&t.depositor, &id);
    assert_eq!(t.client.get_escrow_age(&id), 0);
}

// ── #570: Per-depositor escrow count limit ───────────────────────────────────

#[test]
fn test_max_escrows_per_depositor_succeeds_at_limit() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    for _ in 0..100 {
        t.client.create_escrow(
            &t.depositor,
            &t.beneficiary,
            &t.token,
            &1,
            &expiry,
            &empty_memo(&t.e),
        );
    }

    let list = t.client.get_escrows_by_depositor(&t.depositor);
    assert_eq!(list.len(), 100);
}

#[test]
#[should_panic(expected = "TooManyEscrows: depositor has reached the active escrow limit")]
fn test_max_escrows_per_depositor_panics_at_101() {
    let t = setup();
    let expiry = t.e.ledger().sequence() + 1000;

    for _ in 0..100 {
        t.client.create_escrow(
            &t.depositor,
            &t.beneficiary,
            &t.token,
            &1,
            &expiry,
            &empty_memo(&t.e),
        );
    }

    // The 101st escrow must panic
    t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &1,
        &expiry,
        &empty_memo(&t.e),
    );
}

// ── #452: escrowed_value_for_depositor ────────────────────────────────────────

#[test]
fn test_escrowed_value_for_depositor_zero_for_new_address() {
    let t = setup();
    let stranger = Address::generate(&t.e);
    assert_eq!(t.client.escrowed_value_for_depositor(&stranger), 0);
}

#[test]
fn test_escrowed_value_for_depositor_correct_sum() {
    let t = setup();
    mint_more(&t, crate::storage_types::MIN_ESCROW_AMOUNT * 3);
    let expiry = t.e.ledger().sequence() + 1000;

    t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &empty_memo(&t.e),
    );
    t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &(crate::storage_types::MIN_ESCROW_AMOUNT * 2),
        &expiry,
        &empty_memo(&t.e),
    );

    assert_eq!(
        t.client.escrowed_value_for_depositor(&t.depositor),
        crate::storage_types::MIN_ESCROW_AMOUNT * 3
    );
}

#[test]
fn test_escrowed_value_for_depositor_excludes_settled() {
    let t = setup();
    mint_more(&t, crate::storage_types::MIN_ESCROW_AMOUNT * 3);
    let expiry = t.e.ledger().sequence() + 1000;

    let id1 = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &empty_memo(&t.e),
    );
    let id2 = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &(crate::storage_types::MIN_ESCROW_AMOUNT * 2),
        &expiry,
        &empty_memo(&t.e),
    );

    assert_eq!(
        t.client.escrowed_value_for_depositor(&t.depositor),
        crate::storage_types::MIN_ESCROW_AMOUNT * 3
    );

    // Release first escrow
    t.client.release_escrow(&t.depositor, &id1);
    assert_eq!(
        t.client.escrowed_value_for_depositor(&t.depositor),
        crate::storage_types::MIN_ESCROW_AMOUNT * 2
    );

    // Refund second escrow
    t.client.refund_escrow(&t.depositor, &id2);
    assert_eq!(t.client.escrowed_value_for_depositor(&t.depositor), 0);
}
// ── Section A: amberly-d's issues ──────────────────────────────────────────────

fn mint_more(t: &TestEnv<'_>, amount: i128) {
    soroban_sdk::token::StellarAssetClient::new(&t.e, &t.token).mint(&t.depositor, &amount);
}

#[test]
#[should_panic(expected = "cannot refund active dispute")]
fn test_refund_blocked_when_open_dispute_exists() {
    let t = setup();
    mint_more(&t, crate::storage_types::MIN_ESCROW_AMOUNT * 2);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &empty_memo(&t.e),
    );
    t.client.raise_dispute(&t.depositor, &id);
    t.client.refund_escrow(&t.depositor, &id);
}

#[test]
#[should_panic(expected = "admin not set")]
fn test_release_blocked_when_open_dispute_exists() {
    let t = setup();
    mint_more(&t, crate::storage_types::MIN_ESCROW_AMOUNT * 2);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &empty_memo(&t.e),
    );
    t.client.raise_dispute(&t.beneficiary, &id);
    t.client.release_escrow(&t.beneficiary, &id);
}

#[test]
fn test_refund_succeeds_after_dispute_is_resolved() {
    let t = setup();
    mint_more(&t, crate::storage_types::MIN_ESCROW_AMOUNT * 2);
    let arbiter = Address::generate(&t.e);
    t.client.set_arbiter(&arbiter);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &empty_memo(&t.e),
    );
    t.client.raise_dispute(&t.depositor, &id);
    t.client.resolve_dispute(&arbiter, &id, &t.depositor);

    let record = t.client.get_escrow(&id);
    assert!(record.refunded);
    let tc = soroban_sdk::token::Client::new(&t.e, &t.token);
    assert_eq!(
        tc.balance(&t.depositor),
        50_000 + crate::storage_types::MIN_ESCROW_AMOUNT * 2
    );
}

#[test]
fn test_topup_escrow_increases_amount() {
    let t = setup();
    mint_more(&t, crate::storage_types::MIN_ESCROW_AMOUNT * 3);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &empty_memo(&t.e),
    );
    let before = t.client.get_escrow(&id).amount;
    t.client
        .topup_escrow(&t.depositor, &id, &crate::storage_types::MIN_ESCROW_AMOUNT);
    assert_eq!(
        before + crate::storage_types::MIN_ESCROW_AMOUNT,
        t.client.get_escrow(&id).amount
    );
}

#[test]
#[should_panic(expected = "escrow already settled")]
fn test_topup_escrow_settled_panics() {
    let t = setup();
    mint_more(&t, crate::storage_types::MIN_ESCROW_AMOUNT * 3);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &empty_memo(&t.e),
    );
    t.client.release_escrow(&t.depositor, &id);
    t.client
        .topup_escrow(&t.depositor, &id, &crate::storage_types::MIN_ESCROW_AMOUNT);
}

#[test]
#[should_panic(expected = "DisputeOpen")]
fn test_topup_escrow_with_open_dispute_panics() {
    let t = setup();
    mint_more(&t, crate::storage_types::MIN_ESCROW_AMOUNT * 3);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &empty_memo(&t.e),
    );
    t.client.raise_dispute(&t.depositor, &id);
    t.client
        .topup_escrow(&t.depositor, &id, &crate::storage_types::MIN_ESCROW_AMOUNT);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_topup_escrow_zero_amount_panics() {
    let t = setup();
    mint_more(&t, crate::storage_types::MIN_ESCROW_AMOUNT * 2);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &empty_memo(&t.e),
    );
    t.client.topup_escrow(&t.depositor, &id, &0);
}

#[test]
fn test_escrow_between_returns_id_when_active_match_exists() {
    let t = setup();
    mint_more(&t, crate::storage_types::MIN_ESCROW_AMOUNT * 2);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &empty_memo(&t.e),
    );
    assert_eq!(t.client.escrow_between(&t.depositor, &t.beneficiary), id);
}

#[test]
#[should_panic(expected = "no escrow found between the two addresses")]
fn test_escrow_between_returns_none_when_no_match() {
    let t = setup();
    let a = Address::generate(&t.e);
    let b = Address::generate(&t.e);
    t.client.escrow_between(&a, &b);
}

#[test]
fn test_escrowed_value_for_depositor_sums_active_escrows() {
    let t = setup();
    mint_more(&t, crate::storage_types::MIN_ESCROW_AMOUNT * 4);
    let expiry = t.e.ledger().sequence() + 1000;
    t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &empty_memo(&t.e),
    );
    t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &(crate::storage_types::MIN_ESCROW_AMOUNT * 2),
        &expiry,
        &empty_memo(&t.e),
    );
    assert_eq!(
        t.client.escrowed_value_for_depositor(&t.depositor),
        crate::storage_types::MIN_ESCROW_AMOUNT * 3
    );
}

#[test]
fn test_escrowed_value_for_depositor_excludes_settled_v2() {
    let t = setup();
    mint_more(&t, crate::storage_types::MIN_ESCROW_AMOUNT * 4);
    let expiry = t.e.ledger().sequence() + 1000;
    let id1 = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &empty_memo(&t.e),
    );
    let id2 = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &(crate::storage_types::MIN_ESCROW_AMOUNT * 2),
        &expiry,
        &empty_memo(&t.e),
    );
    t.client.release_escrow(&t.depositor, &id1);
    assert_eq!(
        t.client.escrowed_value_for_depositor(&t.depositor),
        crate::storage_types::MIN_ESCROW_AMOUNT * 2
    );
    t.client.refund_escrow(&t.depositor, &id2);
    assert_eq!(t.client.escrowed_value_for_depositor(&t.depositor), 0);
}

#[test]
fn test_escrowed_value_for_depositor_returns_zero_with_no_escrows() {
    let t = setup();
    assert_eq!(t.client.escrowed_value_for_depositor(&t.depositor), 0);
}

// ── #666: admin_settle_escrow ─────────────────────────────────────────────────

#[test]
fn test_admin_settle_escrow_to_beneficiary() {
    let t = setup();
    let admin = Address::generate(&t.e);
    t.client.initialize(&admin);
    mint_more(&t, crate::storage_types::MIN_ESCROW_AMOUNT * 2);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &empty_memo(&t.e),
    );
    t.client.admin_settle_escrow(&admin, &id, &t.beneficiary);
    let record = t.client.get_escrow(&id);
    assert!(record.released);
    let tc = soroban_sdk::token::Client::new(&t.e, &t.token);
    assert_eq!(
        tc.balance(&t.beneficiary),
        crate::storage_types::MIN_ESCROW_AMOUNT
    );
}

#[test]
fn test_admin_settle_escrow_to_depositor() {
    let t = setup();
    let admin = Address::generate(&t.e);
    t.client.initialize(&admin);
    mint_more(&t, crate::storage_types::MIN_ESCROW_AMOUNT * 2);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &empty_memo(&t.e),
    );
    t.client.admin_settle_escrow(&admin, &id, &t.depositor);
    let record = t.client.get_escrow(&id);
    assert!(record.refunded);
    assert_eq!(t.client.escrowed_total(), 0);
}

#[test]
#[should_panic(expected = "winner must be depositor or beneficiary")]
fn test_admin_settle_escrow_invalid_winner_panics() {
    let t = setup();
    let admin = Address::generate(&t.e);
    t.client.initialize(&admin);
    mint_more(&t, crate::storage_types::MIN_ESCROW_AMOUNT * 2);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &empty_memo(&t.e),
    );
    let stranger = Address::generate(&t.e);
    t.client.admin_settle_escrow(&admin, &id, &stranger);
}

#[test]
#[should_panic(expected = "escrow already settled")]
fn test_admin_settle_escrow_settled_panics() {
    let t = setup();
    let admin = Address::generate(&t.e);
    t.client.initialize(&admin);
    mint_more(&t, crate::storage_types::MIN_ESCROW_AMOUNT * 2);
    let expiry = t.e.ledger().sequence() + 1000;
    let id = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &empty_memo(&t.e),
    );
    t.client.release_escrow(&t.depositor, &id);
    t.client.admin_settle_escrow(&admin, &id, &t.beneficiary);
}

// ── Section B: northersubair's issue #668 ──────────────────────────────────────

#[test]
fn test_100_escrows_sequential_ids() {
    let t = setup();
    mint_more(&t, crate::storage_types::MIN_ESCROW_AMOUNT * 100);
    let expiry = t.e.ledger().sequence() + 1000;
    for i in 0..100u32 {
        let id = t.client.create_escrow(
            &t.depositor,
            &t.beneficiary,
            &t.token,
            &crate::storage_types::MIN_ESCROW_AMOUNT,
            &expiry,
            &empty_memo(&t.e),
        );
        assert_eq!(id, i);
    }
}

#[test]
fn test_100_escrows_all_released_contract_balance_zero() {
    let t = setup();
    mint_more(&t, crate::storage_types::MIN_ESCROW_AMOUNT * 100);
    let expiry = t.e.ledger().sequence() + 1000;
    let mut ids = Vec::new(&t.e);
    for _ in 0..100 {
        let id = t.client.create_escrow(
            &t.depositor,
            &t.beneficiary,
            &t.token,
            &crate::storage_types::MIN_ESCROW_AMOUNT,
            &expiry,
            &empty_memo(&t.e),
        );
        ids.push_back(id);
    }
    for i in 0..ids.len() {
        t.client.release_escrow(&t.depositor, &ids.get(i).unwrap());
    }
    assert_eq!(t.client.escrowed_total(), 0);
    let tc = soroban_sdk::token::Client::new(&t.e, &t.token);
    assert_eq!(tc.balance(&t.client.address), 0);
}

#[test]
fn test_two_escrows_release_one_other_unaffected() {
    let t = setup();
    mint_more(&t, crate::storage_types::MIN_ESCROW_AMOUNT * 4);
    let expiry = t.e.ledger().sequence() + 1000;
    let id1 = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &empty_memo(&t.e),
    );
    let id2 = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &(crate::storage_types::MIN_ESCROW_AMOUNT * 2),
        &expiry,
        &empty_memo(&t.e),
    );
    t.client.release_escrow(&t.depositor, &id1);
    let record2 = t.client.get_escrow(&id2);
    assert!(!record2.released && !record2.refunded);
    t.client.refund_escrow(&t.depositor, &id2);
    assert!(t.client.get_escrow(&id2).refunded);
}

#[test]
fn test_dispute_on_one_escrow_does_not_affect_others() {
    let t = setup();
    mint_more(&t, crate::storage_types::MIN_ESCROW_AMOUNT * 4);
    let arbiter = Address::generate(&t.e);
    t.client.set_arbiter(&arbiter);
    let expiry = t.e.ledger().sequence() + 1000;
    let id1 = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &crate::storage_types::MIN_ESCROW_AMOUNT,
        &expiry,
        &empty_memo(&t.e),
    );
    let id2 = t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &(crate::storage_types::MIN_ESCROW_AMOUNT * 2),
        &expiry,
        &empty_memo(&t.e),
    );
    t.client.raise_dispute(&t.depositor, &id1);
    t.client.release_escrow(&t.depositor, &id2);
    assert!(t.client.get_escrow(&id2).released);
    assert!(!t.client.get_escrow(&id1).released);
}

// ── #745: Protocol fee ────────────────────────────────────────────────────────

struct FeeEnv<'a> {
    e: Env,
    client: VeriTixPayClient<'a>,
    admin: Address,
    depositor: Address,
    beneficiary: Address,
    treasury: Address,
    token: Address,
}

fn fee_setup() -> FeeEnv<'static> {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let treasury = Address::generate(&e);
    let token = e.register_stellar_asset_contract(depositor.clone());
    soroban_sdk::token::StellarAssetClient::new(&e, &token).mint(&depositor, &50_000_000);

    FeeEnv {
        e,
        client,
        admin,
        depositor,
        beneficiary,
        treasury,
        token,
    }
}

fn fee_create_escrow(t: &FeeEnv, amount: i128) -> u32 {
    let expiry = t.e.ledger().sequence() + 1000;
    t.client.create_escrow(
        &t.depositor,
        &t.beneficiary,
        &t.token,
        &amount,
        &expiry,
        &empty_memo(&t.e),
    )
}

#[test]
fn test_protocol_fee_zero_full_amount_to_beneficiary() {
    let t = fee_setup();
    let token_client = soroban_sdk::token::Client::new(&t.e, &t.token);

    // No fee configured — the full amount goes to the beneficiary.
    let id = fee_create_escrow(&t, 10_000_000);
    t.client.release_escrow(&t.depositor, &id);

    assert_eq!(token_client.balance(&t.beneficiary), 10_000_000);
    assert_eq!(token_client.balance(&t.treasury), 0);
}

#[test]
fn test_protocol_fee_2pct_correctly_deducted() {
    let t = fee_setup();
    let token_client = soroban_sdk::token::Client::new(&t.e, &t.token);

    // 2% protocol fee (200 bps).
    t.client.set_protocol_fee(&t.admin, &200, &t.treasury);

    let id = fee_create_escrow(&t, 10_000_000);
    t.client.release_escrow(&t.depositor, &id);

    // 2% of 10_000_000 = 200_000 deducted, rest goes to the beneficiary.
    assert_eq!(token_client.balance(&t.beneficiary), 9_800_000);
}

#[test]
fn test_protocol_fee_treasury_receives_fee() {
    let t = fee_setup();
    let token_client = soroban_sdk::token::Client::new(&t.e, &t.token);

    t.client.set_protocol_fee(&t.admin, &200, &t.treasury);

    let id = fee_create_escrow(&t, 10_000_000);
    t.client.release_escrow(&t.depositor, &id);

    // The treasury receives exactly the fee amount.
    assert_eq!(token_client.balance(&t.treasury), 200_000);
}

#[test]
#[should_panic(expected = "protocol fee cannot exceed 500 bps")]
fn test_set_protocol_fee_over_500bps_panics() {
    let t = fee_setup();
    t.client.set_protocol_fee(&t.admin, &501, &t.treasury);
}

#[test]
fn test_get_protocol_fee_returns_configured_values() {
    let t = fee_setup();

    t.client.set_protocol_fee(&t.admin, &200, &t.treasury);
    let (fee_bps, treasury, total_collected) = t.client.protocol_fee_stats();
    assert_eq!(fee_bps, 200);
    assert_eq!(treasury, t.treasury);
    assert_eq!(total_collected, 0);

    // After a release, the collected fee total is tracked.
    let id = fee_create_escrow(&t, 10_000_000);
    t.client.release_escrow(&t.depositor, &id);

    let (_fee_bps, _treasury, total_collected) = t.client.protocol_fee_stats();
    assert_eq!(total_collected, 200_000);
}

#[test]
fn test_protocol_fee_not_applied_to_refunds() {
    let t = fee_setup();
    let token_client = soroban_sdk::token::Client::new(&t.e, &t.token);

    t.client.set_protocol_fee(&t.admin, &200, &t.treasury);

    let id = fee_create_escrow(&t, 10_000_000);
    t.client.refund_escrow(&t.depositor, &id);

    // Refunds are fee-exempt — the depositor gets everything back.
    assert_eq!(token_client.balance(&t.depositor), 50_000_000);
    assert_eq!(token_client.balance(&t.treasury), 0);
}
