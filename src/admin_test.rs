#![cfg(test)]

use crate::contract::{VeriTixPay, VeriTixPayClient};
use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Address, Env};

struct TestEnv<'a> {
    e: Env,
    client: VeriTixPayClient<'a>,
    admin: Address,
    new_admin: Address,
}

fn setup() -> TestEnv<'static> {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    let new_admin = Address::generate(&e);

    client.initialize(&admin);

    TestEnv {
        e,
        client,
        admin,
        new_admin,
    }
}

#[test]
fn test_transfer_ownership_sets_proposed_admin() {
    let t = setup();
    t.client.transfer_ownership(&t.new_admin);
}

#[test]
#[should_panic(expected = "cannot propose current admin")]
fn test_transfer_ownership_self_proposal_panics() {
    let t = setup();
    t.client.transfer_ownership(&t.admin);
}

#[test]
fn test_accept_admin_sets_new_admin_with_delay() {
    let t = setup();
    t.client.transfer_ownership(&t.new_admin);
    t.client.accept_admin(&t.new_admin);

    let active_after = t.client.admin_active_after_ledger();
    assert!(active_after > t.e.ledger().sequence());
}

#[test]
fn test_admin_active_after_ledger_returns_zero_when_not_set() {
    let e = Env::default();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    assert_eq!(client.admin_active_after_ledger(), 0);
}

#[test]
fn test_old_admin_still_active_during_delay() {
    let t = setup();
    t.client.transfer_ownership(&t.new_admin);
    t.client.accept_admin(&t.new_admin);

    t.client.transfer_ownership(&Address::generate(&t.e));
}

#[test]
fn test_full_ownership_transfer_lifecycle() {
    let t = setup();

    t.client.transfer_ownership(&t.new_admin);
    t.client.accept_admin(&t.new_admin);

    let active_after = t.client.admin_active_after_ledger();
    assert!(active_after > t.e.ledger().sequence());

    t.e.ledger().with_mut(|l| l.sequence_number = active_after);
    t.client.transfer_ownership(&Address::generate(&t.e));
}

// ── #662: admin & pause protection ────────────────────────────────────────────

#[test]
#[should_panic(expected = "Unauthorized: caller is not the contract admin")]
fn test_check_admin_panics_for_non_admin() {
    let t = setup();
    let stranger = Address::generate(&t.e);
    let from = Address::generate(&t.e);
    let token = crate::test::create_token_contract(&t.e, &t.admin);
    soroban_sdk::token::StellarAssetClient::new(&t.e, &token).mint(&from, &1000);

    // Anything that routes through check_admin with a non-admin caller panics.
    t.client.clawback(&stranger, &from, &100);
}

#[test]
fn test_set_clawback_cosigner_stored() {
    let t = setup();
    let cosigner = Address::generate(&t.e);
    t.e.as_contract(&t.client.address, || {
        crate::admin::set_clawback_cosigner(&t.e, &t.admin, &cosigner);
        let stored = crate::admin::read_clawback_cosigner(&t.e);
        assert_eq!(stored, Some(cosigner.clone()));
    });
}

#[test]
fn test_clawback_batch_without_cosigner_succeeds() {
    let t = setup();
    let from = Address::generate(&t.e);
    t.client.mint(&t.admin, &from, &1000);

    let clawbacks = soroban_sdk::vec![&t.e, (from.clone(), 400i128)];
    t.client.clawback_batch(&t.admin, &clawbacks);
    assert_eq!(t.client.balance(&from), 600);
}

#[test]
#[should_panic(expected = "insufficient balance")]
fn test_clawback_insufficient_balance_panics() {
    let t = setup();
    let from = Address::generate(&t.e);
    t.client.mint(&t.admin, &from, &100);

    let clawbacks = soroban_sdk::vec![&t.e, (from.clone(), 200i128)];
    t.client.clawback_batch(&t.admin, &clawbacks);
}

#[test]
fn test_set_paused_by_admin_toggles_state() {
    let t = setup();
    t.client.set_paused(&t.admin, &true);
    assert!(t.client.is_paused());
    t.client.set_paused(&t.admin, &false);
    assert!(!t.client.is_paused());
}

#[test]
#[should_panic(expected = "InvalidFreeze: cannot freeze the admin address")]
fn test_admin_cannot_freeze_itself() {
    let t = setup();
    t.e.as_contract(&t.client.address, || {
        crate::freeze::freeze_account(&t.e, &t.admin, &t.admin);
    });
}
