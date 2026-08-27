#![cfg(test)]

use crate::recurring::{get_recurring_history, record_recurring_execution};
use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Env};

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
    assert_eq!(
        history.get(0).unwrap().execution_ledger,
        e.ledger().sequence()
    );

    // Simulate next execution
    e.ledger()
        .with_mut(|l| l.sequence_number = e.ledger().sequence() + 10);
    record_recurring_execution(e.clone(), caller.clone(), recurring_id, amount);

    let history = get_recurring_history(e.clone(), recurring_id);
    assert_eq!(history.len(), 2);
    assert_eq!(history.get(1).unwrap().amount, amount);
    assert_eq!(
        history.get(1).unwrap().execution_ledger,
        e.ledger().sequence()
    );
}

#[test]
#[should_panic(expected = "recurring is not active")]
fn test_max_executions_deactivates() {
    use crate::recurring::{execute_recurring, setup_recurring};
    use crate::storage_types::DataKey;
    use soroban_sdk::{token, Address};

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
    let mut record: crate::recurring::RecurringRecord = e
        .storage()
        .persistent()
        .get(&DataKey::Recurring(recurring_id))
        .unwrap();
    assert!(record.active);
    assert_eq!(record.execution_count, 0);
    assert_eq!(record.max_executions, 3);

    // 1st execution
    e.ledger()
        .with_mut(|l| l.sequence_number = e.ledger().sequence() + interval);
    execute_recurring(&e, recurring_id);

    // Check state after 1st execution
    record = e
        .storage()
        .persistent()
        .get(&DataKey::Recurring(recurring_id))
        .unwrap();
    assert!(record.active);
    assert_eq!(record.execution_count, 1);

    // 2nd execution
    e.ledger()
        .with_mut(|l| l.sequence_number = e.ledger().sequence() + interval);
    execute_recurring(&e, recurring_id);

    // Check state after 2nd execution
    record = e
        .storage()
        .persistent()
        .get(&DataKey::Recurring(recurring_id))
        .unwrap();
    assert!(record.active);
    assert_eq!(record.execution_count, 2);

    // 3rd execution - this should deactivate the record
    e.ledger()
        .with_mut(|l| l.sequence_number = e.ledger().sequence() + interval);
    execute_recurring(&e, recurring_id);

    // Check state after 3rd execution - should be inactive
    record = e
        .storage()
        .persistent()
        .get(&DataKey::Recurring(recurring_id))
        .unwrap();
    assert!(!record.active);
    assert_eq!(record.execution_count, 3);

    // 4th execution - this should panic with "recurring is not active"
    e.ledger()
        .with_mut(|l| l.sequence_number = e.ledger().sequence() + interval);
    execute_recurring(&e, recurring_id);
}

#[test]
fn test_is_recurring_active() {
    use crate::contract::{VeriTixPay, VeriTixPayClient};
    use soroban_sdk::Address;

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
        &payer, &payee, &token, &100, &100, // interval
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

// ── #676: scheduled drift ─────────────────────────────────────────────────────

#[test]
fn test_delayed_execute_does_not_drift_schedule() {
#[test]
fn test_pause_and_resume_by_payer() {
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

    client.pause_recurring(&payer, &id);
    assert!(!client.is_recurring_active(&id));

    client.resume_recurring(&payer, &id);
    assert!(client.is_recurring_active(&id));
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_pause_recurring_non_payer_panics() {
    use soroban_sdk::{testutils::Address as _, Address};
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register_contract(None, crate::contract::VeriTixPay);
    let client = crate::contract::VeriTixPayClient::new(&e, &contract_id);

    let payer = soroban_sdk::Address::generate(&e);
    let payee = soroban_sdk::Address::generate(&e);
    let token = e.register_stellar_asset_contract(payer.clone());
    soroban_sdk::token::StellarAssetClient::new(&e, &token).mint(&payer, &100_000_000);

    let interval: u32 = 100;
    let id = client.setup_recurring(&payer, &payee, &token, &1000, &interval, &5);

    let read_record = |e: &Env| -> crate::recurring::RecurringRecord {
        e.as_contract(&contract_id, || {
            e.storage()
                .persistent()
                .get(&crate::storage_types::DataKey::Recurring(id))
                .unwrap()
        })
    };

    let start = e.ledger().sequence();
    let record_before: crate::recurring::RecurringRecord = read_record(&e);
    assert_eq!(record_before.last_charged_ledger, start);

    // Execute 110 ledgers later (10 ledgers late).
    e.ledger().with_mut(|l| l.sequence_number = start + 110);
    client.execute_recurring(&id);

    // The schedule must anchor to the baseline, not the late execution ledger.
    let record_after: crate::recurring::RecurringRecord = read_record(&e);
    assert_eq!(record_after.last_charged_ledger, start + 100);

    // Advance to the exact next due (start + 200); still due and executes fine.
    e.ledger().with_mut(|l| l.sequence_number = start + 200);
    client.execute_recurring(&id);

    let record_final: crate::recurring::RecurringRecord = read_record(&e);
    assert_eq!(record_final.last_charged_ledger, start + 200);
    assert_eq!(record_final.execution_count, 2);
}
    let payer = Address::generate(&e);
    let intruder = Address::generate(&e);
    let payee = Address::generate(&e);
    let token = e.register_stellar_asset_contract(Address::generate(&e));
    soroban_sdk::token::StellarAssetClient::new(&e, &token).mint(&payer, &1000);

    let id = client.setup_recurring(&payer, &payee, &token, &100, &100, &5);
    client.pause_recurring(&intruder, &id);
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_resume_recurring_non_payer_panics() {
    use soroban_sdk::{testutils::Address as _, Address};
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register_contract(None, crate::contract::VeriTixPay);
    let client = crate::contract::VeriTixPayClient::new(&e, &contract_id);

    let payer = Address::generate(&e);
    let intruder = Address::generate(&e);
    let payee = Address::generate(&e);
    let token = e.register_stellar_asset_contract(Address::generate(&e));
    soroban_sdk::token::StellarAssetClient::new(&e, &token).mint(&payer, &1000);

    let id = client.setup_recurring(&payer, &payee, &token, &100, &100, &5);
    client.pause_recurring(&payer, &id);

    // A non-payer caller must not be able to resume another payer's recurring payment.
    client.resume_recurring(&intruder, &id);
}

#[cfg(test)]
mod recurring_history_tests {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn test_recurring_execution_audit_log() {
        let env = Env::default();
        env.mock_all_auths();

        let recurring_id = 1;
        let amount = 5000_i128;

        // Record a successful execution
        record_execution(&env, recurring_id, amount);

        // Fetch history via contract view
        let history = VeritixContract::get_recurring_history(env.clone(), recurring_id);
        
        assert_eq!(history.len(), 1);
        let execution = history.get(0).unwrap();
        assert_eq!(execution.recurring_id, recurring_id);
        assert_eq!(execution.amount, amount);
    }
}
#[cfg(test)]
mod payee_recurring_tests {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn test_get_recurring_by_payee_indexing() {
        let env = Env::default();
        env.mock_all_auths();

        let payee = Address::generate(&env);
        let recurring_id = 42;

        // Index the recurring payment for the payee
        index_recurring_for_payee(&env, &payee, recurring_id);

        // Fetch recurring IDs via contract view
        let payee_recurrings = VeritixContract::get_recurring_by_payee(env.clone(), payee.clone());
        
        assert_eq!(payee_recurrings.len(), 1);
        assert_eq!(payee_recurrings.get(0).unwrap(), recurring_id);

        // Remove recurring payment and verify index update
        remove_recurring_for_payee(&env, &payee, recurring_id);
        let updated_recurrings = VeritixContract::get_recurring_by_payee(env, payee);
        assert_eq!(updated_recurrings.len(), 0);
    }
}