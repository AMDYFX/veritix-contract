#![cfg(test)]

// ── #732: Event emission coverage ────────────────────────────────────────────
// Off-chain indexers depend entirely on contract events, so every function that
// calls `e.events().publish` gets at least one test asserting the event appears
// with the correct topic symbol.
//
// Functions that do NOT publish events (initialize, mint, burn, clawback,
// freeze, unfreeze, pause, unpause, distribute, cancel_split) have no event
// assertions here by design.

use crate::contract::{VeriTixPay, VeriTixPayClient};
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    token, Address, Bytes, Env, Symbol, Vec,
};

fn setup() -> (
    Env,
    VeriTixPayClient<'static>,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let token = e.register_stellar_asset_contract(depositor.clone());
    soroban_sdk::token::StellarAssetClient::new(&e, &token).mint(&depositor, &50_000_000);

    (
        e,
        client,
        admin,
        depositor,
        beneficiary,
        token,
        Address::generate(&e),
        contract_id,
    )
}

fn has_event(e: &Env, symbol: &str) -> bool {
    e.events().all().events().iter().any(|ev| {
        ev.topics()
            .get(0)
            .map(|t| t == Symbol::new(e, symbol).into())
            .unwrap_or(false)
    })
}

// ── Escrow events ────────────────────────────────────────────────────────────

#[test]
fn test_create_escrow_emits_event() {
    let (e, client, _admin, depositor, beneficiary, token, _arbiter, _contract_id) = setup();
    let expiry = e.ledger().sequence() + 1000;
    client.create_escrow(
        &depositor,
        &beneficiary,
        &token,
        &10_000_000,
        &expiry,
        &Bytes::new(&e),
    );
    assert!(has_event(&e, "escrow_cr"));
}

#[test]
fn test_release_escrow_emits_event() {
    let (e, client, _admin, depositor, beneficiary, token, _arbiter, _contract_id) = setup();
    let expiry = e.ledger().sequence() + 1000;
    let id = client.create_escrow(
        &depositor,
        &beneficiary,
        &token,
        &10_000_000,
        &expiry,
        &Bytes::new(&e),
    );
    client.release_escrow(&depositor, &id);
    assert!(has_event(&e, "escrow_rl"));
}

#[test]
fn test_refund_escrow_emits_event() {
    let (e, client, _admin, depositor, beneficiary, token, _arbiter, _contract_id) = setup();
    let expiry = e.ledger().sequence() + 1000;
    let id = client.create_escrow(
        &depositor,
        &beneficiary,
        &token,
        &10_000_000,
        &expiry,
        &Bytes::new(&e),
    );
    client.refund_escrow(&depositor, &id);
    assert!(has_event(&e, "escrow_rf"));
}

// ── Dispute events ───────────────────────────────────────────────────────────

#[test]
fn test_raise_dispute_emits_event() {
    let (e, client, _admin, depositor, beneficiary, token, _arbiter, _contract_id) = setup();
    let expiry = e.ledger().sequence() + 1000;
    let id = client.create_escrow(
        &depositor,
        &beneficiary,
        &token,
        &10_000_000,
        &expiry,
        &Bytes::new(&e),
    );
    client.raise_dispute(&depositor, &id);
    assert!(has_event(&e, "dispute"));
}

#[test]
fn test_resolve_dispute_emits_event() {
    let (e, client, _admin, depositor, beneficiary, token, arbiter, _contract_id) = setup();
    client.set_arbiter(&arbiter);

    let expiry = e.ledger().sequence() + 1000;
    let id = client.create_escrow(
        &depositor,
        &beneficiary,
        &token,
        &10_000_000,
        &expiry,
        &Bytes::new(&e),
    );
    client.raise_dispute(&depositor, &id);
    client.resolve_dispute(&arbiter, &id, &beneficiary);
    assert!(has_event(&e, "dis_res"));
}

#[test]
fn test_appeal_dispute_emits_event() {
    let (e, client, _admin, depositor, beneficiary, token, _arbiter, _contract_id) = setup();
    let expiry = e.ledger().sequence() + 1000;
    let id = client.create_escrow(
        &depositor,
        &beneficiary,
        &token,
        &10_000_000,
        &expiry,
        &Bytes::new(&e),
    );
    client.raise_dispute(&depositor, &id);
    client.appeal_dispute(&depositor, &id);
    assert!(has_event(&e, "appeal"));
}

#[test]
fn test_resolve_appeal_emits_event() {
    let (e, client, _admin, depositor, beneficiary, token, arbiter, _contract_id) = setup();
    client.set_arbiter(&arbiter);

    let expiry = e.ledger().sequence() + 1000;
    let id = client.create_escrow(
        &depositor,
        &beneficiary,
        &token,
        &10_000_000,
        &expiry,
        &Bytes::new(&e),
    );
    client.raise_dispute(&depositor, &id);
    client.appeal_dispute(&depositor, &id);
    client.resolve_appeal(&arbiter, &id, &beneficiary);
    assert!(has_event(&e, "app_res"));
}

#[test]
fn test_expire_dispute_emits_event() {
    let (e, client, _admin, depositor, beneficiary, token, _arbiter, _contract_id) = setup();
    let expiry = e.ledger().sequence() + 1000;
    let id = client.create_escrow(
        &depositor,
        &beneficiary,
        &token,
        &10_000_000,
        &expiry,
        &Bytes::new(&e),
    );
    client.raise_dispute(&depositor, &id);

    // Wait past the expiry window (DISPUTE_EXPIRE_AFTER = 34560 ledgers).
    e.ledger().with_mut(|l| l.sequence_number += 34_561);
    client.expire_dispute(&depositor, &id);
    assert!(has_event(&e, "dis_exp"));
}

// ── Token / transfer events ──────────────────────────────────────────────────

#[test]
fn test_transfer_emits_event() {
    let (e, client, _admin, _depositor, _beneficiary, token, _arbiter, _contract_id) = setup();
    let token_admin = token::StellarAssetClient::new(&e, &token);
    let from = Address::generate(&e);
    let to = Address::generate(&e);
    token_admin.mint(&from, &1000);
    token_admin.mint(&to, &1000);

    client.transfer_with_memo(&from, &to, &100, &Bytes::new(&e));
    assert!(has_event(&e, "transfer"));
}

#[test]
fn test_approve_batch_emits_event() {
    let (e, client, _admin, _depositor, _beneficiary, _token, _arbiter, _contract_id) = setup();
    let from = Address::generate(&e);
    let spender = Address::generate(&e);

    let mut approvals = Vec::new(&e);
    approvals.push_back((spender, 500i128, 1000u32));

    client.approve_batch(&from, &approvals);
    assert!(has_event(&e, "approve"));
}

// ── Split events ─────────────────────────────────────────────────────────────

#[test]
fn test_create_split_emits_event() {
    let (e, _client, _admin, _depositor, _beneficiary, token, _arbiter, _contract_id) = setup();
    let sender = Address::generate(&e);
    soroban_sdk::token::StellarAssetClient::new(&e, &token).mint(&sender, &10_000);

    let recipient1 = Address::generate(&e);
    let recipient2 = Address::generate(&e);
    let recipients = Vec::from_array(&e, [(recipient1, 6000u32), (recipient2, 4000u32)]);
    let event_ledger = e.ledger().sequence() + 1000;

    let _split_id =
        crate::splitter::create_split(e.clone(), sender, recipients, token, 1000, event_ledger);
    assert!(has_event(&e, "split_cr"));
}

// ── Recurring events ─────────────────────────────────────────────────────────

#[test]
fn test_setup_recurring_emits_event() {
    let (e, client, _admin, _depositor, _beneficiary, token, _arbiter, _contract_id) = setup();
    let payer = Address::generate(&e);
    let payee = Address::generate(&e);
    soroban_sdk::token::StellarAssetClient::new(&e, &token).mint(&payer, &1000);

    client.setup_recurring(&payer, &payee, &token, &100, &100, &5);
    assert!(has_event(&e, "rcr_set"));
}

#[test]
fn test_execute_recurring_emits_event() {
    let (e, client, _admin, _depositor, _beneficiary, token, _arbiter, _contract_id) = setup();
    let payer = Address::generate(&e);
    let payee = Address::generate(&e);
    soroban_sdk::token::StellarAssetClient::new(&e, &token).mint(&payer, &1000);

    let id = client.setup_recurring(&payer, &payee, &token, &100, &100, &5);
    e.ledger().with_mut(|l| l.sequence_number += 100);
    client.execute_recurring(&id);
    assert!(has_event(&e, "rcr_exec"));
}

#[test]
fn test_cancel_recurring_emits_event() {
    let (e, client, _admin, _depositor, _beneficiary, token, _arbiter, _contract_id) = setup();
    let payer = Address::generate(&e);
    let payee = Address::generate(&e);
    soroban_sdk::token::StellarAssetClient::new(&e, &token).mint(&payer, &1000);

    let id = client.setup_recurring(&payer, &payee, &token, &100, &100, &5);
    client.cancel_recurring(&payer, &id);
    assert!(has_event(&e, "rcr_cnl"));
}

// ── Admin / governance events ────────────────────────────────────────────────

#[test]
fn test_emergency_withdraw_emits_event() {
    let (e, client, admin, _depositor, _beneficiary, token, _arbiter, contract_id) = setup();
    let token_admin = token::StellarAssetClient::new(&e, &token);
    let recipient = Address::generate(&e);
    token_admin.mint(&contract_id, &1000);

    client.emergency_withdraw(&admin, &recipient, &token, &1000);
    assert!(has_event(&e, "em_wdraw"));
}

#[test]
fn test_take_snapshot_emits_event() {
    let (e, client, admin, _depositor, _beneficiary, _token, _arbiter, _contract_id) = setup();
    let user = Address::generate(&e);
    client.take_snapshot(&admin, &user);
    assert!(has_event(&e, "snapshot"));
}

#[test]
fn test_transfer_ownership_emits_event() {
    let (e, client, _admin, _depositor, _beneficiary, _token, _arbiter, _contract_id) = setup();
    let new_admin = Address::generate(&e);
    client.transfer_ownership(&new_admin);
    assert!(has_event(&e, "ownership"));
}

#[test]
fn test_admin_settle_escrow_emits_event() {
    let (e, client, admin, depositor, beneficiary, token, _arbiter, _contract_id) = setup();
    let expiry = e.ledger().sequence() + 1000;
    let id = client.create_escrow(
        &depositor,
        &beneficiary,
        &token,
        &10_000_000,
        &expiry,
        &Bytes::new(&e),
    );
    client.admin_settle_escrow(&admin, &id, &beneficiary);
    assert!(has_event(&e, "esc_stl"));
}

#[test]
fn test_split_to_escrow_emits_event() {
    let (e, client, _admin, _depositor, _beneficiary, token, _arbiter, _contract_id) = setup();
    let sender = Address::generate(&e);
    soroban_sdk::token::StellarAssetClient::new(&e, &token).mint(&sender, &100_000_000);

    let recipient1 = Address::generate(&e);
    let recipient2 = Address::generate(&e);
    let recipients = Vec::from_array(&e, [(recipient1, 5000u32), (recipient2, 5000u32)]);
    let expiry = e.ledger().sequence() + 1000;

    client.split_to_escrow(&sender, &recipients, &token, &10_000_000, &expiry);
    assert!(has_event(&e, "split_esc"));
}

#[test]
fn test_airdrop_emits_event() {
    let (e, client, admin, _depositor, _beneficiary, token, _arbiter, _contract_id) = setup();
    let token_admin = token::StellarAssetClient::new(&e, &token);

    // Populate the holder set through internal mints.
    let h1 = Address::generate(&e);
    let h2 = Address::generate(&e);
    client.mint(&admin, &h1, &100);
    client.mint(&admin, &h2, &100);

    // Give the admin and holders external token balances for the airdrop.
    token_admin.mint(&admin, &1000);
    token_admin.mint(&h1, &700);
    token_admin.mint(&h2, &300);

    client.airdrop(&admin, &token, &100);
    assert!(has_event(&e, "airdrop"));
}
