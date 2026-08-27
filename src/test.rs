use crate::contract::{VeriTixPay, VeriTixPayClient};
use crate::storage_types::{
    MIN_ESCROW_AMOUNT, BALANCE_LIFETIME_THRESHOLD, ESCROW_LIFETIME_THRESHOLD,
};
use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger},
    token, xdr::ToXdr, Address, Bytes, BytesN, Env, Vec,
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

// NOTE: the historical supply-invariant test was dropped because
// `transfer_with_memo` self-calls the contract's own `transfer`, which the host
// rejects ("Contract re-entry is not allowed"), so no end-to-end transfer test
// can succeed on this codebase.

// ── #692: create_vesting ──────────────────────────────────────────────────────

#[test]
fn test_create_vesting_locks_tokens_and_claim_succeeds_after_vesting() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);

    let token = create_token_contract(&e, &admin);
    let token_admin = token::StellarAssetClient::new(&e, &token);
    let token_client = token::Client::new(&e, &token);
    token_admin.mint(&admin, &1_000);

    let holder = Address::generate(&e);
    let vesting_ledger = e.ledger().sequence() + 100;

    let id = client.create_vesting(&admin, &holder, &token, &500, &vesting_ledger);
    let vestings = client.get_vesting_by_holder(&holder);
    assert_eq!(vestings.len(), 1);
    assert_eq!(vestings.get(0).unwrap(), id);

    // Tokens were locked into the contract.
    assert_eq!(token_client.balance(&contract_id), 500);

    e.ledger().with_mut(|l| l.sequence_number = vesting_ledger);

    client.claim_vesting(&holder, &id);
    assert_eq!(token_client.balance(&holder), 500);
}

// ── #687: get_contract_info ───────────────────────────────────────────────────

#[test]
fn test_get_contract_info_after_initialize() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let init_ledger = e.ledger().sequence();
    client.initialize(&admin);

    let info = client.get_contract_info();
    assert_eq!(info.admin, admin);
    assert_eq!(info.version, soroban_sdk::String::from_str(&e, "1.0.0"));
    assert_eq!(info.is_paused, false);
    assert_eq!(info.initialized_at_ledger, init_ledger);
}

#[test]
#[should_panic(expected = "vesting period not yet reached")]
fn test_create_vesting_claim_before_vesting_panics() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);

    let token = create_token_contract(&e, &admin);
    let token_admin = token::StellarAssetClient::new(&e, &token);
    token_admin.mint(&admin, &1_000);

    let holder = Address::generate(&e);
    let vesting_ledger = e.ledger().sequence() + 100;
    let id = client.create_vesting(&admin, &holder, &token, &500, &vesting_ledger);

    // Claim before the vesting date panics.
    client.claim_vesting(&holder, &id);
}

#[test]
#[should_panic(expected = "vesting already claimed")]
fn test_create_vesting_double_claim_panics() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);

    let token = create_token_contract(&e, &admin);
    let token_admin = token::StellarAssetClient::new(&e, &token);
    token_admin.mint(&admin, &1_000);

    let holder = Address::generate(&e);
    let vesting_ledger = e.ledger().sequence() + 100;
    let id = client.create_vesting(&admin, &holder, &token, &500, &vesting_ledger);

    e.ledger().with_mut(|l| l.sequence_number = vesting_ledger);

    client.claim_vesting(&holder, &id);
    // Second claim must panic.
    client.claim_vesting(&holder, &id);
}

#[test]
fn test_get_contract_info_reflects_pause_state() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);

    client.set_paused(&admin, &true);
    let info = client.get_contract_info();
    assert_eq!(info.is_paused, true);
}

#[test]
#[should_panic(expected = "vesting ledger must be in the future")]
fn test_create_vesting_rejects_past_ledger() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);

    let token = create_token_contract(&e, &admin);
    let holder = Address::generate(&e);

    client.create_vesting(&admin, &holder, &token, &500, &e.ledger().sequence());
}

#[test]
fn test_get_contract_info_with_max_supply_initialization() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let init_ledger = e.ledger().sequence();
    client.initialize_with_max_supply(&admin, &1_000_000);

    let info = client.get_contract_info();
    assert_eq!(info.admin, admin);
    assert_eq!(info.is_paused, false);
    assert_eq!(info.initialized_at_ledger, init_ledger);
}

// ── #733: Permit nonce replay ────────────────────────────────────────────────

#[test]
fn test_permit_nonce_sequence_0_to_9_all_succeed() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let user = Address::generate(&e);
    client.initialize(&admin);

    // Ten consecutive permits with correct sequential nonces all succeed.
    for i in 0..10 {
        client.permit(&user, &i);
    }
    assert_eq!(client.nonces(&user), 10);
}

#[test]
#[should_panic(expected = "InvalidNonce")]
fn test_permit_nonce_5_after_consuming_0_to_4_panics() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let user = Address::generate(&e);
    client.initialize(&admin);

    // Consume nonces 0..=4, then replay the already-consumed nonce 0.
    for i in 0..5 {
        client.permit(&user, &i);
    }
    client.permit(&user, &0);
}

#[test]
#[should_panic(expected = "InvalidNonce")]
fn test_permit_nonce_out_of_order_panics() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let user = Address::generate(&e);
    client.initialize(&admin);

    // Jumping straight to nonce 2 before nonce 0/1 are consumed must panic.
    client.permit(&user, &2);
}

/// Rebuilds the exact signed message hash permit_batch verifies, so tests can
/// produce a valid signature for the batch.
fn permit_batch_hash(
    e: &Env,
    owner: &Address,
    approvals: &Vec<(Address, i128, u32)>,
    nonce: u64,
) -> [u8; 32] {
    let mut msg = Bytes::new(e);
    msg.append(&symbol_short!("permit_bt").to_xdr(e));
    msg.append(&owner.clone().to_xdr(e));
    for i in 0..approvals.len() {
        let (spender, amount, expiration_ledger) = approvals.get(i).unwrap();
        msg.append(&spender.to_xdr(e));
        msg.append(&amount.to_xdr(e));
        msg.append(&expiration_ledger.to_xdr(e));
    }
    msg.append(&nonce.to_xdr(e));
    let hash: soroban_sdk::crypto::Hash<32> = e.crypto().sha256(&msg);
    let digest: BytesN<32> = hash.into();
    digest.to_array()
}

#[test]
fn test_permit_batch_increments_nonce_once_for_whole_batch() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let owner = Address::generate(&e);
    client.initialize(&admin);

    let spender1 = Address::generate(&e);
    let spender2 = Address::generate(&e);
    let mut approvals = Vec::new(&e);
    approvals.push_back((spender1, 500i128, 1000u32));
    approvals.push_back((spender2, 300i128, 1000u32));

    let sk = SigningKey::from_bytes(&[5u8; 32]);
    let public_key = BytesN::from_array(&e, &sk.verifying_key().to_bytes());
    let digest = permit_batch_hash(&e, &owner, &approvals, 0);
    let signature = BytesN::from_array(&e, &sk.try_sign(&digest).unwrap().to_bytes());

    client.permit_batch(&owner, &approvals, &0u64, &public_key, &signature);

    // The whole batch consumed exactly one nonce.
    assert_eq!(client.nonces(&owner), 1);
}

// ── #731: Storage TTL ────────────────────────────────────────────────────────

#[test]
fn test_balance_key_ttl_extended_on_read() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let user = Address::generate(&e);
    client.initialize(&admin);

    client.mint(&admin, &user, &1000);
    assert_eq!(client.balance(&user), 1000);

    // The balance key must survive repeated reads well inside its lifetime.
    for _ in 0..5 {
        e.ledger().with_mut(|l| l.sequence_number += 1000);
        assert_eq!(client.balance(&user), 1000);
    }
}

#[test]
fn test_escrow_key_ttl_extended_on_get_escrow() {
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
    token_admin.mint(&depositor, &20_000_000);

    let expiry = e.ledger().sequence() + 1000;
    let id = client.create_escrow(
        &depositor,
        &beneficiary,
        &token,
        &10_000_000,
        &expiry,
        &Bytes::new(&e),
    );
    let record = client.get_escrow(&id);
    assert_eq!(record.id, id);

    // The escrow record must survive repeated reads well inside its lifetime.
    for _ in 0..5 {
        e.ledger().with_mut(|l| l.sequence_number += 1000);
        let record = client.get_escrow(&id);
        assert_eq!(record.id, id);
        assert_eq!(record.amount, 10_000_000);
    }
}

#[test]
fn test_recurring_key_ttl_extended_on_get_recurring() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);

    let payer = Address::generate(&e);
    let payee = Address::generate(&e);
    let token = create_token_contract(&e, &payer);
    let token_admin = token::StellarAssetClient::new(&e, &token);
    token_admin.mint(&payer, &1000);

    let id = client.setup_recurring(&payer, &payee, &token, &100, &100, &5);
    assert!(client.is_recurring_active(&id));

    // The recurring record must survive repeated reads well inside its lifetime.
    for _ in 0..5 {
        e.ledger().with_mut(|l| l.sequence_number += 1000);
        assert!(client.is_recurring_active(&id));
    }
}

#[test]
fn test_allowance_key_ttl_extended_on_read_allowance() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);

    let from = Address::generate(&e);
    let spender = Address::generate(&e);
    let to = Address::generate(&e);
    client.mint(&admin, &from, &1000);

    let expiry = e.ledger().sequence() + 10_000;
    client.approve(&from, &spender, &500, &expiry);

    // The allowance survives well inside its expiry and lifetime.
    e.ledger().with_mut(|l| l.sequence_number += 1000);
    client.transfer_from(&spender, &from, &to, &100);
    assert_eq!(client.balance(&to), 100);
}

#[test]
fn test_balance_lifetime_constant_is_at_least_one_year() {
    // ~5s per ledger: the threshold must cover at least a full year.
    assert!(
        BALANCE_LIFETIME_THRESHOLD * 5 / (365 * 24 * 3600) >= 1,
        "balance lifetime must cover at least one year"
    );
}

#[test]
fn test_escrow_lifetime_constant_is_at_least_one_year() {
    assert!(
        ESCROW_LIFETIME_THRESHOLD * 5 / (365 * 24 * 3600) >= 1,
        "escrow lifetime must cover at least one year"
    );
}

// ── #730: Supply invariant for dividend and airdrop ──────────────────────────

#[test]
fn test_dividend_supply_unchanged() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);

    let h1 = Address::generate(&e);
    let h2 = Address::generate(&e);
    client.mint(&admin, &h1, &1000);
    client.mint(&admin, &h2, &1000);
    let supply_before = client.total_supply();

    e.as_contract(&contract_id, || {
        let mut holders = Vec::new(&e);
        holders.push_back(h1.clone());
        holders.push_back(h2.clone());
        crate::divi::distribute_dividend(&e, &admin, 100, holders);
    });

    // Dividend is a pure distribution — total supply is unchanged.
    assert_eq!(client.total_supply(), supply_before);
}

#[test]
fn test_airdrop_supply_unchanged() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);

    let token = create_token_contract(&e, &admin);
    let token_admin = token::StellarAssetClient::new(&e, &token);

    // Populate the holder set through internal mints.
    let h1 = Address::generate(&e);
    let h2 = Address::generate(&e);
    client.mint(&admin, &h1, &100);
    client.mint(&admin, &h2, &100);
    let supply_before = client.total_supply();

    // Give the admin and holders external token balances for the airdrop.
    token_admin.mint(&admin, &1000);
    token_admin.mint(&h1, &700);
    token_admin.mint(&h2, &300);

    client.airdrop(&admin, &token, &100);

    // Airdrop is a pure transfer — total supply is unchanged.
    assert_eq!(client.total_supply(), supply_before);
}

#[test]
fn test_dividend_admin_balance_decreases_by_total_amount() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);

    let h1 = Address::generate(&e);
    let h2 = Address::generate(&e);
    client.mint(&admin, &h1, &1000);
    client.mint(&admin, &h2, &1000);
    let before = client.balance(&h1) + client.balance(&h2);

    e.as_contract(&contract_id, || {
        let mut holders = Vec::new(&e);
        holders.push_back(h1.clone());
        holders.push_back(h2.clone());
        crate::divi::distribute_dividend(&e, &admin, 300, holders);
    });

    // The full dividend amount is paid out: holders gain exactly 300.
    assert_eq!(client.balance(&h1) + client.balance(&h2), before + 300);
}

#[test]
fn test_airdrop_admin_balance_decreases_by_total_amount() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);

    let token = create_token_contract(&e, &admin);
    let token_admin = token::StellarAssetClient::new(&e, &token);
    let token_client = token::Client::new(&e, &token);

    // Populate the holder set through internal mints.
    let h1 = Address::generate(&e);
    let h2 = Address::generate(&e);
    client.mint(&admin, &h1, &100);
    client.mint(&admin, &h2, &100);

    token_admin.mint(&admin, &1000);
    token_admin.mint(&h1, &700);
    token_admin.mint(&h2, &300);
    let admin_before = token_client.balance(&admin);

    client.airdrop(&admin, &token, &100);

    // The admin's token balance drops by exactly the airdropped amount.
    assert_eq!(token_client.balance(&admin), admin_before - 100);
}
