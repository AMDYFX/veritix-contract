#![cfg(test)]

use crate::contract::{VeriTixPay, VeriTixPayClient};
use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Address, Env};

struct TestEnv<'a> {
    e: Env,
    contract_id: Address,
    client: VeriTixPayClient<'a>,
    admin: Address,
}

fn setup() -> TestEnv<'static> {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    TestEnv {
        e,
        contract_id,
        client,
        admin,
    }
}

fn freeze(e: &Env, contract_id: &Address, admin: &Address, user: &Address) {
    e.as_contract(contract_id, || {
        crate::freeze::freeze_account(e, admin, user)
    });
}

fn unfreeze(e: &Env, contract_id: &Address, admin: &Address, user: &Address) {
    e.as_contract(contract_id, || {
        crate::freeze::unfreeze_account(e, admin, user)
    });
}

fn is_frozen(e: &Env, contract_id: &Address, user: &Address) -> bool {
    e.as_contract(contract_id, || crate::freeze::is_frozen(e, user))
}

#[test]
fn test_freeze_account_stores_true() {
    let t = setup();
    let user = Address::generate(&t.e);
    freeze(&t.e, &t.contract_id, &t.admin, &user);
    assert!(is_frozen(&t.e, &t.contract_id, &user));
    assert!(t.client.spendable_balance(&user) == 0);
}

#[test]
fn test_unfreeze_removes_key() {
    let t = setup();
    let user = Address::generate(&t.e);
    freeze(&t.e, &t.contract_id, &t.admin, &user);
    assert!(is_frozen(&t.e, &t.contract_id, &user));

    unfreeze(&t.e, &t.contract_id, &t.admin, &user);
    assert!(!is_frozen(&t.e, &t.contract_id, &user));
}

#[test]
#[should_panic(expected = "AlreadyFrozen: account is already frozen")]
fn test_freeze_already_frozen_panics() {
    let t = setup();
    let user = Address::generate(&t.e);
    freeze(&t.e, &t.contract_id, &t.admin, &user);
    freeze(&t.e, &t.contract_id, &t.admin, &user);
}

#[test]
#[should_panic(expected = "NotFrozen: account is not frozen")]
fn test_unfreeze_not_frozen_panics() {
    let t = setup();
    let user = Address::generate(&t.e);
    unfreeze(&t.e, &t.contract_id, &t.admin, &user);
}

#[test]
#[should_panic(expected = "InvalidFreeze: cannot freeze the admin address")]
fn test_freeze_admin_address_panics() {
    let t = setup();
    freeze(&t.e, &t.contract_id, &t.admin, &t.admin);
}

#[test]
fn test_frozen_account_cannot_transfer() {
    let t = setup();
    let user = Address::generate(&t.e);
    t.client.mint(&t.admin, &user, &1000);
    assert_eq!(t.client.spendable_balance(&user), 1000);

    freeze(&t.e, &t.contract_id, &t.admin, &user);
    assert_eq!(t.client.spendable_balance(&user), 0);
}

#[test]
fn test_frozen_account_can_receive_mint() {
    let t = setup();
    let user = Address::generate(&t.e);
    freeze(&t.e, &t.contract_id, &t.admin, &user);

    // Freezing blocks spending, not receiving.
    t.client.mint(&t.admin, &user, &500);
    assert_eq!(t.client.balance(&user), 500);
    assert_eq!(t.client.spendable_balance(&user), 0);
}

#[test]
fn test_unfreeze_restores_spendable_balance() {
    let t = setup();
    let user = Address::generate(&t.e);
    t.client.mint(&t.admin, &user, &1000);
    freeze(&t.e, &t.contract_id, &t.admin, &user);
    assert_eq!(t.client.spendable_balance(&user), 0);

    unfreeze(&t.e, &t.contract_id, &t.admin, &user);
    assert_eq!(t.client.spendable_balance(&user), 1000);
}

// ── #743: freeze_until ────────────────────────────────────────────────────────

fn freeze_until(e: &Env, contract_id: &Address, admin: &Address, user: &Address, until: u32) {
    e.as_contract(contract_id, || {
        crate::freeze::freeze_until(e, admin, user, until)
    });
}

#[test]
fn test_freeze_until_blocks_transfer_before_expiry() {
    let t = setup();
    let user = Address::generate(&t.e);
    t.client.mint(&t.admin, &user, &1000);

    let until = t.e.ledger().sequence() + 100;
    freeze_until(&t.e, &t.contract_id, &t.admin, &user, until);

    // Frozen until the target ledger — spending is blocked.
    assert_eq!(t.client.spendable_balance(&user), 0);
}

#[test]
fn test_freeze_until_auto_clears_at_expiry_ledger() {
    let t = setup();
    let user = Address::generate(&t.e);
    let until = t.e.ledger().sequence() + 100;
    freeze_until(&t.e, &t.contract_id, &t.admin, &user, until);
    assert!(is_frozen(&t.e, &t.contract_id, &user));

    // Advance past the expiry ledger — the freeze auto-clears.
    t.e.ledger().with_mut(|l| l.sequence_number = until);
    assert!(!is_frozen(&t.e, &t.contract_id, &user));
}

#[test]
#[should_panic(expected = "InvalidFreezeUntil: until_ledger must be in the future")]
fn test_freeze_until_past_ledger_panics() {
    let t = setup();
    let user = Address::generate(&t.e);
    freeze_until(&t.e, &t.contract_id, &t.admin, &user, t.e.ledger().sequence());
}

#[test]
fn test_freeze_until_manual_unfreeze_before_expiry_succeeds() {
    let t = setup();
    let user = Address::generate(&t.e);
    let until = t.e.ledger().sequence() + 100;
    freeze_until(&t.e, &t.contract_id, &t.admin, &user, until);
    assert!(is_frozen(&t.e, &t.contract_id, &user));

    // Manual unfreeze before the expiry ledger works.
    unfreeze(&t.e, &t.contract_id, &t.admin, &user);
    assert!(!is_frozen(&t.e, &t.contract_id, &user));
}

#[test]
fn test_freeze_until_cleared_account_can_transfer() {
    let t = setup();
    let user = Address::generate(&t.e);
    t.client.mint(&t.admin, &user, &1000);

    let until = t.e.ledger().sequence() + 100;
    freeze_until(&t.e, &t.contract_id, &t.admin, &user, until);
    assert_eq!(t.client.spendable_balance(&user), 0);

    // Once the expiry ledger passes, the account can spend again.
    t.e.ledger().with_mut(|l| l.sequence_number = until);
    assert_eq!(t.client.spendable_balance(&user), 1000);
}
