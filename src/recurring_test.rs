#![cfg(test)]

use soroban_sdk::{Env, testutils::Address as _, testutils::Ledger as _};
use crate::recurring::{record_recurring_execution, get_recurring_history};

#[test]
fn test_recurring_history_grows() {
    let e = Env::default();
    e.mock_all_auths();

    let caller = soroban_sdk::Address::generate(&e);
    let recurring_id = 1;
    let amount = 500;
    
    record_recurring_execution(e.clone(), caller.clone(), recurring_id, amount);
    
    let history = get_recurring_history(e.clone(), recurring_id);
    assert_eq!(history.len(), 1);
    assert_eq!(history.get(0).unwrap().amount, amount);
    assert_eq!(history.get(0).unwrap().execution_ledger, e.ledger().sequence());
    
    // Simulate next execution
    e.ledger().with_mut(|l| l.sequence_number = e.ledger().sequence() + 10);
    record_recurring_execution(e.clone(), caller.clone(), recurring_id, amount);
    
    let history = get_recurring_history(e.clone(), recurring_id);
    assert_eq!(history.len(), 2);
    assert_eq!(history.get(1).unwrap().amount, amount);
    assert_eq!(history.get(1).unwrap().execution_ledger, e.ledger().sequence());
}

#[test]
#[should_panic(expected = "recurring is not active")]
fn test_max_executions_deactivates() {
    use soroban_sdk::{Address, token};
    use crate::recurring::{setup_recurring, execute_recurring};
    use crate::storage_types::DataKey;

    let e = Env::default();
    e.mock_all_auths();

    let payer = Address::generate(&e);
    let payee = Address::generate(&e);
    // Create a test token
    let token = e.register_stellar_asset_contract(Address::generate(&e));
    let _token_client = token::Client::new(&e, &token);
    
    // Mint some tokens to the payer so transfers work
    soroban_sdk::token::StellarAssetClient::new(&e, &token).mint(&payer, &1000);
    
    let amount = 100;
    let interval = 100; // 100 ledgers between executions
    let max_executions = 3;

    // Setup recurring payment with max 3 executions
    let recurring_id = setup_recurring(
        &e,
        payer.clone(),
        payee.clone(),
        token.clone(),
        amount,
        interval,
        max_executions,
    );

    // Verify initial state
    let mut record: crate::recurring::RecurringRecord = e.storage().persistent().get(&DataKey::Recurring(recurring_id)).unwrap();
    assert!(record.active);
    assert_eq!(record.execution_count, 0);
    assert_eq!(record.max_executions, 3);

    // 1st execution
    e.ledger().with_mut(|l| l.sequence_number = e.ledger().sequence() + interval);
    execute_recurring(&e, recurring_id);
    
    // Check state after 1st execution
    record = e.storage().persistent().get(&DataKey::Recurring(recurring_id)).unwrap();
    assert!(record.active);
    assert_eq!(record.execution_count, 1);

    // 2nd execution
    e.ledger().with_mut(|l| l.sequence_number = e.ledger().sequence() + interval);
    execute_recurring(&e, recurring_id);
    
    // Check state after 2nd execution
    record = e.storage().persistent().get(&DataKey::Recurring(recurring_id)).unwrap();
    assert!(record.active);
    assert_eq!(record.execution_count, 2);

    // 3rd execution - this should deactivate the record
    e.ledger().with_mut(|l| l.sequence_number = e.ledger().sequence() + interval);
    execute_recurring(&e, recurring_id);
    
    // Check state after 3rd execution - should be inactive
    record = e.storage().persistent().get(&DataKey::Recurring(recurring_id)).unwrap();
    assert!(!record.active);
    assert_eq!(record.execution_count, 3);

    // 4th execution - this should panic with "recurring is not active"
    e.ledger().with_mut(|l| l.sequence_number = e.ledger().sequence() + interval);
    execute_recurring(&e, recurring_id);
}

#[test]
fn test_is_recurring_active() {
    use soroban_sdk::Address;
    use crate::contract::{VeriTixPay, VeriTixPayClient};

    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);

    let payer = Address::generate(&e);
    let payee = Address::generate(&e);
    let token = e.register_stellar_asset_contract(payer.clone());
    soroban_sdk::token::StellarAssetClient::new(&e, &token).mint(&payer, &1000);

    // Non-existent recurring should return false
    assert!(!client.is_recurring_active(&999));

    // Setup a new recurring payment
    let recurring_id = client.setup_recurring(
        &payer,
        &payee,
        &token,
        &100,
        &100, // interval
        &3,   // max executions
    );

    // Should be active after creation
    assert!(client.is_recurring_active(&recurring_id));

    // Execute all max executions to deactivate
    for _i in 1..=3 {
        e.ledger().with_mut(|l| l.sequence_number += 100);
        client.execute_recurring(&recurring_id);
    }

    // Should be inactive after max executions
    assert!(!client.is_recurring_active(&recurring_id));
}

#[test]
fn test_cancel_recurring_removes_from_payer_index() {
    use soroban_sdk::{testutils::Address as _, Address};
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register_contract(None, crate::contract::VeriTixPay);
    let client = crate::contract::VeriTixPayClient::new(&e, &contract_id);

    let payer = Address::generate(&e);
    let payee = Address::generate(&e);
    let token = e.register_stellar_asset_contract(Address::generate(&e));
    soroban_sdk::token::StellarAssetClient::new(&e, &token).mint(&payer, &1000);

    let id = client.setup_recurring(&payer, &payee, &token, &100, &100, &5);
    let list_before = client.get_recurring_by_payer(&payer);
    assert_eq!(list_before.len(), 1);

    client.cancel_recurring(&payer, &id);
    let list_after = client.get_recurring_by_payer(&payer);
    assert_eq!(list_after.len(), 0);
}