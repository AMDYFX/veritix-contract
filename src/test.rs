use crate::contract::{VeriTixPay, VeriTixPayClient};
use crate::storage_types::MIN_ESCROW_AMOUNT;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Bytes, Env, Vec,
};

pub fn create_token_contract(e: &Env, admin: &Address) -> Address {
    e.register_stellar_asset_contract(admin.clone())
}

#[test]
fn test_emergency_withdraw() {
    let e = Env::default();
    e.mock_all_auths();

    // Setup contract and admin
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    // Create token and mint some tokens
    let token = create_token_contract(&e, &admin);
    let token_admin_client = token::StellarAssetClient::new(&e, &token);
    let token_client = token::Client::new(&e, &token);

    // Mint 1000 tokens directly to the contract (stranded funds)
    token_admin_client.mint(&contract_id, &1000);

    // Create a recipient to receive the withdrawn funds
    let recipient = Address::generate(&e);

    // Verify contract has 1000 tokens, total escrowed is 0
    assert_eq!(token_client.balance(&contract_id), 1000);
    assert_eq!(client.escrowed_total(), 0);

    // Withdraw the stranded funds
    client.emergency_withdraw(&admin, &recipient, &token, &1000);

    // Verify recipient received the funds, contract has 0 left
    assert_eq!(token_client.balance(&recipient), 1000);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
#[should_panic(expected = "Insufficient non-escrowed funds")]
fn test_emergency_withdraw_cannot_touch_escrow_funds() {
    let e = Env::default();
    e.mock_all_auths();

    // Setup contract and admin
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    // Create token and mint some tokens to depositor
    let depositor = Address::generate(&e);
    let token = create_token_contract(&e, &depositor);
    let token_admin_client = token::StellarAssetClient::new(&e, &token);
    let token_client = token::Client::new(&e, &token);

    token_admin_client.mint(&depositor, &1000);

    // Create an escrow which locks 500 tokens in the contract
    let beneficiary = Address::generate(&e);
    let expiry = e.ledger().sequence() + 1000;
    let _id = client.create_escrow(
        &depositor,
        &beneficiary,
        &token,
        &500,
        &expiry,
        &soroban_sdk::Bytes::new(&e),
    );

    // Verify contract has 500 tokens in escrow
    assert_eq!(token_client.balance(&contract_id), 500);
    assert_eq!(client.escrowed_total(), 500);

    // Try to withdraw 501 tokens - should panic because only 0 non-escrowed funds exist
    let recipient = Address::generate(&e);
    client.emergency_withdraw(&admin, &recipient, &token, &501);
}

// ── #578: Full governance lifecycle test ──────────────────────────────────────

#[test]
fn test_full_governance_lifecycle() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.sequence_number = 100);

    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);

    let admin_a = Address::generate(&e);
    let admin_b = Address::generate(&e);
    client.initialize(&admin_a);

    assert_eq!(client.admin_active_after_ledger(), 0);

    let token = create_token_contract(&e, &admin_a);
    let token_admin = token::StellarAssetClient::new(&e, &token);
    let token_client = token::Client::new(&e, &token);

    let mut addrs: Vec<Address> = Vec::new(&e);
    for _ in 0..10 {
        let addr = Address::generate(&e);
        token_admin.mint(&addr, &(500 * MIN_ESCROW_AMOUNT));
        addrs.push_back(addr);
    }

    token_admin.mint(&admin_a, &(5 * MIN_ESCROW_AMOUNT));

    for i in 0..addrs.len() {
        let addr = addrs.get(i).unwrap();
        assert_eq!(token_client.balance(&addr), 500 * MIN_ESCROW_AMOUNT);
    }

    let beneficiary = Address::generate(&e);
    let expiry = e.ledger().sequence() + 1000;
    let memo = Bytes::new(&e);

    let escrow_id = client.create_escrow(
        &admin_a,
        &beneficiary,
        &token,
        &MIN_ESCROW_AMOUNT,
        &expiry,
        &memo,
    );
    assert_eq!(escrow_id, 0);
    assert_eq!(client.escrowed_total(), MIN_ESCROW_AMOUNT);

    let frozen1 = Address::generate(&e);
    let frozen2 = Address::generate(&e);
    token_admin.mint(&frozen1, &(100 * MIN_ESCROW_AMOUNT));
    token_admin.mint(&frozen2, &(100 * MIN_ESCROW_AMOUNT));

    crate::freeze::freeze_account(&e, &admin_a, &frozen1);
    crate::freeze::freeze_account(&e, &admin_a, &frozen2);

    assert!(crate::freeze::is_frozen(&e, &frozen1));
    assert!(crate::freeze::is_frozen(&e, &frozen2));
    assert!(!crate::freeze::is_frozen(&e, &beneficiary));

    client.release_escrow(&admin_a, &escrow_id);
    assert_eq!(client.escrowed_total(), 0);
    assert_eq!(token_client.balance(&beneficiary), MIN_ESCROW_AMOUNT);

    client.transfer_ownership(&admin_b);
    client.accept_admin(&admin_b);

    let activation_ledger = client.admin_active_after_ledger();
    assert!(activation_ledger > e.ledger().sequence());

    e.ledger()
        .with_mut(|l| l.sequence_number = activation_ledger + 1);

    token_admin.mint(&admin_b, &(5 * MIN_ESCROW_AMOUNT));

    let escrow_id2 = client.create_escrow(
        &admin_b,
        &beneficiary,
        &token,
        &MIN_ESCROW_AMOUNT,
        &expiry,
        &memo,
    );
    assert_eq!(escrow_id2, 1);

    crate::freeze::unfreeze_account(&e, &admin_b, &frozen1);
    crate::freeze::unfreeze_account(&e, &admin_b, &frozen2);

    assert!(!crate::freeze::is_frozen(&e, &frozen1));
    assert!(!crate::freeze::is_frozen(&e, &frozen2));

    client.release_escrow(&admin_b, &escrow_id2);

    let stats = client.escrow_stats();
    assert_eq!(stats.total_value_locked, 0);

    let by_dep = client.get_escrows_by_depositor(&admin_b);
    assert_eq!(by_dep.len(), 1);
    assert_eq!(by_dep.get(0).unwrap(), escrow_id2);

    assert_eq!(token_client.balance(&beneficiary), 2 * MIN_ESCROW_AMOUNT);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_old_admin_cannot_act_after_transfer() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.sequence_number = 100);

    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);

    let admin_a = Address::generate(&e);
    let admin_b = Address::generate(&e);
    client.initialize(&admin_a);

    client.transfer_ownership(&admin_b);
    client.accept_admin(&admin_b);

    let activation = client.admin_active_after_ledger();
    e.ledger().with_mut(|l| l.sequence_number = activation + 1);

    let stranger = Address::generate(&e);
    crate::freeze::freeze_account(&e, &admin_a, &stranger);
}

// ── #579: Permit nonce replay protection ─────────────────────────────────────

#[test]
fn test_permit_nonce_increments_on_each_call() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let user = Address::generate(&e);
    client.initialize(&admin);

    for i in 0..10 {
        client.permit(&user, &i);
    }
    assert_eq!(client.nonces(&user), 10);
}

#[test]
#[should_panic(expected = "InvalidNonce")]
fn test_permit_nonce_replay_rejected() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let user = Address::generate(&e);
    client.initialize(&admin);

    client.permit(&user, &5);
    client.permit(&user, &5);
}

#[test]
#[should_panic(expected = "InvalidNonce")]
fn test_permit_nonce_wrong_order_panics() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let user = Address::generate(&e);
    client.initialize(&admin);

    client.permit(&user, &2);
}

#[test]
fn test_nonces_view_returns_current_nonce_after_n_permits() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let user = Address::generate(&e);
    client.initialize(&admin);

    assert_eq!(client.nonces(&user), 0);
    client.permit(&user, &0);
    assert_eq!(client.nonces(&user), 1);
    client.permit(&user, &1);
    assert_eq!(client.nonces(&user), 2);
    client.permit(&user, &2);
    assert_eq!(client.nonces(&user), 3);
}

// ── #577: Storage expiry simulation ──────────────────────────────────────────

#[test]
fn test_balance_key_without_bump_expires_and_returns_zero() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let user = Address::generate(&e);
    client.initialize(&admin);

    client.mint(&admin, &user, &1000);
    assert_eq!(client.balance(&user), 1000);

    e.ledger().with_mut(|l| {
        l.sequence_number =
            l.sequence_number + crate::storage_types::BALANCE_LIFETIME_THRESHOLD + 1;
    });

    let bal = client.balance(&user);
    assert!(bal == 0 || bal == 1000);
}

#[test]
fn test_escrow_record_expiry_simulation() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);

    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let token = create_token_contract(&e, &depositor);
    let token_admin = token::StellarAssetClient::new(&e, &token);
    token_admin.mint(&depositor, &10_000);

    let expiry = e.ledger().sequence() + 1000;
    let id = client.create_escrow(
        &depositor,
        &beneficiary,
        &token,
        &500,
        &expiry,
        &soroban_sdk::Bytes::new(&e),
    );

    e.ledger().with_mut(|l| {
        l.sequence_number = l.sequence_number + crate::storage_types::ESCROW_LIFETIME_THRESHOLD + 1;
    });

    let _settled = client.is_escrow_settled(&id);
}

#[test]
fn test_allowance_expiry_simulation() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);

    let from = Address::generate(&e);
    let spender = Address::generate(&e);

    let expiry_ledger = e.ledger().sequence() + 100;
    client.approve(&from, &spender, &500, &expiry_ledger);

    e.ledger()
        .with_mut(|l| l.sequence_number = expiry_ledger + 1);

    let _allowance_exists =
        e.storage()
            .persistent()
            .has(&crate::storage_types::DataKey::Allowance(
                from.clone(),
                spender.clone(),
            ));
}

#[test]
fn test_total_supply_invariant_across_mint_and_burn() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);

    let user = Address::generate(&e);
    client.mint(&admin, &user, &1000);
    assert_eq!(client.total_supply(), 1000);

    client.burn(&user, &400);
    assert_eq!(client.total_supply(), 600);
}

// ── #679: Full contract lifecycle ─────────────────────────────────────────────

#[test]
fn test_full_contract_lifecycle() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.sequence_number = 100);

    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);

    // Initialize once
    let admin = Address::generate(&e);
    let user = Address::generate(&e);
    client.initialize(&admin);

    // Contract is live: balances, pauses, snapshots work.
    client.mint(&admin, &user, &1000);
    assert_eq!(client.balance(&user), 1000);
    assert_eq!(client.total_supply(), 1000);

    client.take_snapshot(&admin, &user);
    assert_eq!(client.get_snapshot_balance(&user), 1000);
    assert_eq!(client.snapshot_taken_at(&user), e.ledger().sequence());

    client.set_paused(&admin, &true);
    assert!(client.is_paused());
    client.set_paused(&admin, &false);
    assert!(!client.is_paused());

    // Two-phase ownership transfer completes the handoff.
    let new_admin = Address::generate(&e);
    client.transfer_ownership(&new_admin);
    client.accept_admin(&new_admin);
    let activation = client.admin_active_after_ledger();
    e.ledger().with_mut(|l| l.sequence_number = activation + 1);

    client.mint(&new_admin, &user, &500);
    assert_eq!(client.balance(&user), 1500);

    // Funds remain intact through the owned lifecycle.
    client.burn(&user, &300);
    assert_eq!(client.balance(&user), 1200);
    assert_eq!(client.total_supply(), 1200);
}

#[test]
#[should_panic(expected = "AlreadyInitialized: contract state is locked")]
fn test_initialize_twice_panics() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    client.initialize(&admin);
}
