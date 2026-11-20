//! Execution of authorised recurring charges and retained execution history.

use soroban_sdk::{token, Address, Env, Vec};

use crate::balance;
use crate::storage_types::{DataKey, RecurringExecution, RecurringPayment};
use crate::validation::require_positive_amount;

/// Maximum number of successful charges retained for each schedule.
pub const MAX_RECURRING_HISTORY: u32 = 100;

/// Maximum number of recurring schedules indexed per address (payer or payee).
pub const MAX_RECURRING_PER_ADDRESS: u32 = 100;

/// Creates a recurring schedule and indexes it for both participants.
pub fn create(
    e: &Env,
    payer: &soroban_sdk::Address,
    payee: &soroban_sdk::Address,
    token: &soroban_sdk::Address,
    amount: i128,
    interval_ledgers: u32,
) -> u64 {
    payer.require_auth();
    require_positive_amount(amount);
    if payer == payee {
        panic!("RecurringSameParty: payer and payee must be different addresses");
    }
    if interval_ledgers == 0 {
        panic!("RecurringIntervalInvalid: interval must be positive");
    }

    let next_execution = e
        .ledger()
        .sequence()
        .checked_add(interval_ledgers)
        .unwrap_or_else(|| panic!("RecurringLedgerOverflow: next execution ledger overflows"));
    let id: u64 = e
        .storage()
        .persistent()
        .get(&DataKey::RecurringCount)
        .unwrap_or(0);
    let next_id = id
        .checked_add(1)
        .unwrap_or_else(|| panic!("RecurringIdOverflow: schedule id overflows"));
    let schedule = RecurringPayment {
        payer: payer.clone(),
        payee: payee.clone(),
        token: token.clone(),
        amount,
        interval_ledgers,
        next_execution,
        active: true,
        paused: false,
    };

    e.storage()
        .persistent()
        .set(&DataKey::Recurring(id), &schedule);
    e.storage()
        .persistent()
        .set(&DataKey::RecurringCount, &next_id);

    let mut payer_ids = recurring_ids_for_payer(e, payer);
    if payer_ids.len() >= MAX_RECURRING_PER_ADDRESS {
        panic!("RecurringIndexFull: payer has reached the recurring schedule limit");
    }
    payer_ids.push_back(id);
    e.storage()
        .persistent()
        .set(&DataKey::PayerRecurrings(payer.clone()), &payer_ids);

    let mut payee_ids = recurring_ids_for_payee(e, payee);
    if payee_ids.len() >= MAX_RECURRING_PER_ADDRESS {
        panic!("RecurringIndexFull: payee has reached the recurring schedule limit");
    }
    payee_ids.push_back(id);
    e.storage()
        .persistent()
        .set(&DataKey::PayeeRecurrings(payee.clone()), &payee_ids);

    id
}

/// Reads a recurring schedule by id.
pub fn get_recurring(e: &Env, recurring_id: u64) -> RecurringPayment {
    e.storage()
        .persistent()
        .get::<_, RecurringPayment>(&DataKey::Recurring(recurring_id))
        .unwrap_or_else(|| panic!("RecurringNotFound: payment {} does not exist", recurring_id))
}

/// Whether the recurring schedule at `recurring_id` is active and not paused.
pub fn is_recurring_active(e: &Env, recurring_id: u64) -> bool {
    let schedule = get_recurring(e, recurring_id);
    schedule.active && !schedule.paused
}

/// Returns all schedules created by `payer`.
pub fn get_recurring_by_payer(e: &Env, payer: &soroban_sdk::Address) -> Vec<RecurringPayment> {
    let ids = recurring_ids_for_payer(e, payer);
    let mut schedules = Vec::new(e);
    for id in ids {
        if let Some(schedule) = e
            .storage()
            .persistent()
            .get::<_, RecurringPayment>(&DataKey::Recurring(id))
        {
            schedules.push_back(schedule);
        }
    }
    schedules
}

fn recurring_ids_for_payer(e: &Env, payer: &soroban_sdk::Address) -> Vec<u64> {
    e.storage()
        .persistent()
        .get(&DataKey::PayerRecurrings(payer.clone()))
        .unwrap_or_else(|| Vec::new(e))
}

/// Returns the recurring schedule ids associated with `payee`.
pub fn recurring_ids_for_payee(e: &Env, payee: &soroban_sdk::Address) -> Vec<u64> {
    e.storage()
        .persistent()
        .get(&DataKey::PayeeRecurrings(payee.clone()))
        .unwrap_or_else(|| Vec::new(e))
}

/// Returns the number of schedules indexed for `payee`.
pub fn recurring_count_for_payee(e: &Env, payee: &soroban_sdk::Address) -> u32 {
    recurring_ids_for_payee(e, payee).len()
}

/// Pulls one due interval from the payer and schedules the next interval.
///
/// # Panics
///
/// With `RecurringNotFound`, `RecurringNotDue`, `RecurringInactive`,
/// `RecurringPaused`, `RecurringIntervalInvalid`, or `RecurringLedgerOverflow`
/// when the schedule cannot be executed. The balance transfer also panics with
/// `InsufficientBalance` when the payer cannot cover the charge.
pub fn execute_recurring(e: &Env, recurring_id: u64) {
    let key = DataKey::Recurring(recurring_id);
    let mut schedule = e
        .storage()
        .persistent()
        .get::<_, RecurringPayment>(&key)
        .unwrap_or_else(|| panic!("RecurringNotFound: payment {} does not exist", recurring_id));

    if !schedule.active {
        panic!("RecurringInactive: payment {} is inactive", recurring_id);
    }
    if schedule.paused {
        panic!("RecurringPaused: payment {} is paused", recurring_id);
    }
    if e.ledger().sequence() < schedule.next_execution {
        panic!(
            "RecurringNotDue: payment {} is due at ledger {}",
            recurring_id, schedule.next_execution
        );
    }
    if schedule.interval_ledgers == 0 {
        panic!("RecurringIntervalInvalid: interval must be positive");
    }

    let current_ledger = e.ledger().sequence();
    let next_execution = current_ledger
        .checked_add(schedule.interval_ledgers)
        .unwrap_or_else(|| panic!("RecurringLedgerOverflow: next execution ledger overflows"));
    require_positive_amount(schedule.amount);
    if schedule.token == e.current_contract_address() {
        balance::transfer(e, &schedule.payer, &schedule.payee, schedule.amount);
    } else {
        token::TokenClient::new(e, &schedule.token).transfer_from(
            &e.current_contract_address(),
            &schedule.payer,
            &schedule.payee,
            &schedule.amount,
        );
    }

    schedule.next_execution = next_execution;
    e.storage().persistent().set(&key, &schedule);
    record_execution(e, recurring_id, schedule.amount);
}

/// Pauses an active recurring schedule. Must be called by payer or payee.
pub fn pause_recurring(e: &Env, caller: &Address, recurring_id: u64) {
    caller.require_auth();
    let key = DataKey::Recurring(recurring_id);
    let mut schedule = e
        .storage()
        .persistent()
        .get::<_, RecurringPayment>(&key)
        .unwrap_or_else(|| panic!("RecurringNotFound: payment {} does not exist", recurring_id));

    if caller != &schedule.payer && caller != &schedule.payee {
        panic!("Unauthorized: caller is neither payer nor payee");
    }
    if !schedule.active {
        panic!("RecurringInactive: payment {} is inactive", recurring_id);
    }
    schedule.paused = true;
    e.storage().persistent().set(&key, &schedule);
}

/// Resumes a paused recurring schedule. Must be called by payer or payee.
///
/// If `next_execution` is in the past or current ledger, advances it by one
/// interval from the current ledger so resumption does not trigger a back-charge burst.
pub fn resume_recurring(e: &Env, caller: &Address, recurring_id: u64) {
    caller.require_auth();
    let key = DataKey::Recurring(recurring_id);
    let mut schedule = e
        .storage()
        .persistent()
        .get::<_, RecurringPayment>(&key)
        .unwrap_or_else(|| panic!("RecurringNotFound: payment {} does not exist", recurring_id));

    if caller != &schedule.payer && caller != &schedule.payee {
        panic!("Unauthorized: caller is neither payer nor payee");
    }
    if !schedule.active {
        panic!("RecurringInactive: payment {} is inactive", recurring_id);
    }
    schedule.paused = false;
    let current_ledger = e.ledger().sequence();
    if schedule.next_execution <= current_ledger {
        schedule.next_execution = current_ledger
            .checked_add(schedule.interval_ledgers)
            .unwrap_or_else(|| panic!("RecurringLedgerOverflow: next execution ledger overflows"));
    }
    e.storage().persistent().set(&key, &schedule);
}

/// Records a successful charge, keeping only the newest entries.
///
/// Call this only after the token transfer has succeeded.
pub fn record_execution(e: &Env, recurring_id: u64, amount: i128) {
    let key = DataKey::RecurringHistory(recurring_id);
    let mut history = e
        .storage()
        .persistent()
        .get::<_, Vec<RecurringExecution>>(&key)
        .unwrap_or_else(|| Vec::new(e));

    while history.len() >= MAX_RECURRING_HISTORY {
        history.remove(0);
    }
    history.push_back(RecurringExecution {
        ledger: e.ledger().sequence(),
        amount,
    });
    e.storage().persistent().set(&key, &history);
}

/// Returns the retained successful charge history for `recurring_id`.
///
/// Returns an empty vector if no charge has been recorded for the schedule.
pub fn get_recurring_history(e: &Env, recurring_id: u64) -> Vec<RecurringExecution> {
    e.storage()
        .persistent()
        .get(&DataKey::RecurringHistory(recurring_id))
        .unwrap_or_else(|| Vec::new(e))
}

/// Cancels an active recurring schedule. Must be called by payer or payee.
///
/// Sets `active = false`, removes the schedule from both payer and payee indexes,
/// and emits a `RecurringCancelled` event.
pub fn cancel_recurring(e: &Env, caller: &soroban_sdk::Address, recurring_id: u64) {
    caller.require_auth();
    let key = DataKey::Recurring(recurring_id);
    let mut schedule = e
        .storage()
        .persistent()
        .get::<_, RecurringPayment>(&key)
        .unwrap_or_else(|| panic!("RecurringNotFound: payment {} does not exist", recurring_id));

    if caller != &schedule.payer && caller != &schedule.payee {
        panic!("Unauthorized: caller is neither payer nor payee");
    }
    if !schedule.active {
        panic!("RecurringInactive: payment {} is inactive", recurring_id);
    }

    schedule.active = false;
    e.storage().persistent().set(&key, &schedule);

    let payer_key = DataKey::PayerRecurrings(schedule.payer.clone());
    if let Some(payer_ids) = e.storage().persistent().get::<_, Vec<u64>>(&payer_key) {
        let mut new_ids = Vec::new(e);
        for id in payer_ids.iter() {
            if id != recurring_id {
                new_ids.push_back(id);
            }
        }
        e.storage().persistent().set(&payer_key, &new_ids);
    }

    let payee_key = DataKey::PayeeRecurrings(schedule.payee.clone());
    if let Some(payee_ids) = e.storage().persistent().get::<_, Vec<u64>>(&payee_key) {
        let mut new_ids = Vec::new(e);
        for id in payee_ids.iter() {
            if id != recurring_id {
                new_ids.push_back(id);
            }
        }
        e.storage().persistent().set(&payee_key, &new_ids);
    }

    crate::events::RecurringCancelled {
        caller: caller.clone(),
        recurring_id,
    }
    .publish(e);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::balance::balance_of;
    use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};
    use soroban_sdk::{contract, contractimpl, Address, Env};

    fn setup(
        next_execution: u32,
        active: bool,
        paused: bool,
        payer_balance: i128,
    ) -> (Env, Address, Address, Address) {
        let e = Env::default();
        let contract_id = Address::generate(&e);
        let payer = Address::generate(&e);
        let payee = Address::generate(&e);
        e.ledger().set(LedgerInfo {
            protocol_version: 28,
            sequence_number: 100,
            ..Default::default()
        });
        e.as_contract(&contract_id, || {
            e.storage().persistent().set(
                &DataKey::Recurring(1),
                &RecurringPayment {
                    payer: payer.clone(),
                    payee: payee.clone(),
                    token: contract_id.clone(),
                    amount: 25,
                    interval_ledgers: 10,
                    next_execution,
                    active,
                    paused,
                },
            );
            balance::credit(&e, &payer, payer_balance);
        });
        (e, contract_id, payer, payee)
    }

    #[test]
    fn execution_transfers_one_charge_and_advances_one_interval() {
        let (e, contract_id, payer, payee) = setup(100, true, false, 100);

        e.as_contract(&contract_id, || execute_recurring(&e, 1));

        e.as_contract(&contract_id, || {
            let schedule: RecurringPayment = e
                .storage()
                .persistent()
                .get(&DataKey::Recurring(1))
                .unwrap();
            assert_eq!(schedule.next_execution, 110);
            assert_eq!(balance_of(&e, &payer), 75);
            assert_eq!(balance_of(&e, &payee), 25);
            let history = get_recurring_history(&e, 1);
            assert_eq!(history.len(), 1);
            assert_eq!(history.get(0).unwrap().amount, 25);
            assert_eq!(history.get(0).unwrap().ledger, 100);
        });
    }

    #[test]
    #[should_panic(expected = "RecurringNotDue")]
    fn late_schedule_advances_from_current_ledger_and_burst_is_rejected() {
        // Due at 50, but executed late at ledger 100
        let (e, contract_id, _, _) = setup(50, true, false, 100);
        e.as_contract(&contract_id, || {
            execute_recurring(&e, 1);
            // After execution at ledger 100, next execution is 110. Second immediate execution must fail.
            execute_recurring(&e, 1);
        });
    }

    #[test]
    fn pause_and_resume_recurring_lifecycle() {
        let (e, contract_id, payer, payee) = setup(50, true, false, 100);
        e.as_contract(&contract_id, || {
            pause_recurring(&e, &payer, 1);
            let s: RecurringPayment = e
                .storage()
                .persistent()
                .get(&DataKey::Recurring(1))
                .unwrap();
            assert!(s.paused);

            resume_recurring(&e, &payee, 1);
            let s: RecurringPayment = e
                .storage()
                .persistent()
                .get(&DataKey::Recurring(1))
                .unwrap();
            assert!(!s.paused);
            // next_execution was 50 (in the past); resumed at ledger 100, so next_execution is 100 + 10 = 110
            assert_eq!(s.next_execution, 110);
        });
    }

    #[test]
    #[should_panic(expected = "RecurringNotDue")]
    fn execution_before_due_ledger_is_rejected() {
        let (e, contract_id, _, _) = setup(101, true, false, 100);
        e.as_contract(&contract_id, || execute_recurring(&e, 1));
    }

    #[test]
    #[should_panic(expected = "RecurringInactive")]
    fn inactive_schedule_is_rejected() {
        let (e, contract_id, _, _) = setup(100, false, false, 100);
        e.as_contract(&contract_id, || execute_recurring(&e, 1));
    }

    #[test]
    #[should_panic(expected = "RecurringPaused")]
    fn paused_schedule_is_rejected() {
        let (e, contract_id, _, _) = setup(100, true, true, 100);
        e.as_contract(&contract_id, || execute_recurring(&e, 1));
    }

    #[test]
    #[should_panic(expected = "InsufficientBalance")]
    fn insufficient_payer_balance_is_rejected() {
        let (e, contract_id, _, _) = setup(100, true, false, 24);
        e.as_contract(&contract_id, || execute_recurring(&e, 1));
    }

    #[contract]
    pub struct HistoryHarness;

    #[contractimpl]
    impl HistoryHarness {
        pub fn record(e: Env, recurring_id: u64, amount: i128) {
            record_execution(&e, recurring_id, amount);
        }

        pub fn history(e: Env, recurring_id: u64) -> Vec<RecurringExecution> {
            get_recurring_history(&e, recurring_id)
        }
    }

    #[test]
    fn history_retains_only_the_newest_executions() {
        let e = Env::default();
        let contract_id = e.register_contract(None, HistoryHarness);
        let client = HistoryHarnessClient::new(&e, &contract_id);

        assert_eq!(client.history(&9).len(), 0);
        for amount in 0..=MAX_RECURRING_HISTORY {
            client.record(&9, &(amount as i128));
        }

        let history = client.history(&9);
        assert_eq!(history.len(), MAX_RECURRING_HISTORY);
        assert_eq!(history.get(0).unwrap().amount, 1);
        assert_eq!(history.get(MAX_RECURRING_HISTORY - 1).unwrap().amount, 100);
    }
}
