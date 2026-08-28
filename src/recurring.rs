use crate::storage_types::{DataKey, RecurringExecution, RecurringPayment};
use soroban_sdk::{contracttype, token, Address, Env, Vec};

#[contracttype]
#[derive(Clone)]
pub struct RecurringRecord {
    pub payer: Address,
    pub payee: Address,
    pub token: Address,
    pub amount: i128,
    pub interval: u32,
    pub last_charged_ledger: u32,
    pub active: bool,
    pub max_executions: u32,
    pub execution_count: u32,
}

pub fn setup_recurring(
    e: &Env,
    payer: Address,
    payee: Address,
    token_addr: Address,
    amount: i128,
    interval: u32,
    max_executions: u32,
) -> u32 {
    // #426: amount must be positive — first check
    assert!(amount > 0, "amount must be positive");
    payer.require_auth();

    let id: u32 = e
        .storage()
        .persistent()
        .get(&DataKey::RecurringCount)
        .unwrap_or(0);
    let record = RecurringRecord {
        payer,
        payee,
        token: token_addr,
        amount,
        interval,
        last_charged_ledger: e.ledger().sequence(),
        active: true,
        max_executions,
        execution_count: 0,
    };
    e.storage()
        .persistent()
        .set(&DataKey::Recurring(id), &record);
    e.storage()
        .persistent()
        .set(&DataKey::RecurringCount, &(id + 1));

    let index_key = DataKey::PayerRecurrings(record.payer.clone());
    let mut payer_ids: Vec<u32> = e
        .storage()
        .persistent()
        .get(&index_key)
        .unwrap_or(Vec::new(e));
    payer_ids.push_back(id);
    e.storage().persistent().set(&index_key, &payer_ids);

    // #732: emit a setup event for off-chain indexers
    e.events().publish(
        (
            soroban_sdk::symbol_short!("rcr_set"),
            record.payer.clone(),
            record.payee.clone(),
        ),
        (id, record.amount),
    );

    id
}

pub fn execute_recurring(e: &Env, recurring_id: u32) {
    let mut record: RecurringRecord = e
        .storage()
        .persistent()
        .get(&DataKey::Recurring(recurring_id))
        .expect("recurring not found");

    // Check if the record is active before proceeding
    assert!(record.active, "recurring is not active");

    // #435: due-date check is FIRST — cheapest possible early exit for griefing protection
    let next_due = record
        .last_charged_ledger
        .checked_add(record.interval)
        .expect("overflow");
    assert!(e.ledger().sequence() >= next_due, "not yet due");

    // #749: if an execution window is configured, refuse executions that happen
    // too far past the due date (e.g. a keeper bot that was down for a month).
    let execution_window: u32 = e
        .storage()
        .persistent()
        .get(&DataKey::RecurringExecutionWindow)
        .unwrap_or(0);
    if execution_window > 0 {
        let deadline = next_due.checked_add(execution_window).expect("overflow");
        if e.ledger().sequence() > deadline {
            panic!("ExecutionWindowExpired: payment opportunity has passed");
        }
    }

    record.payer.require_auth();

    let token_client = token::Client::new(e, &record.token);
    token_client.transfer(&record.payer, &record.payee, &record.amount);

    // Anchor schedule to original baseline (not current ledger — prevents drift)
    record.last_charged_ledger = next_due;
    // Increment execution count after successful transfer
    record.execution_count += 1;
    // If we've reached max executions, deactivate the record
    if record.max_executions > 0 && record.execution_count >= record.max_executions {
        record.active = false;
    }
    e.storage()
        .persistent()
        .set(&DataKey::Recurring(recurring_id), &record);

    // #732: emit an execution event for off-chain indexers
    e.events().publish(
        (
            soroban_sdk::symbol_short!("rcr_exec"),
            record.payer.clone(),
            record.payee.clone(),
        ),
        (recurring_id, record.amount),
    );
}

pub fn record_recurring_execution(e: Env, caller: Address, recurring_id: u32, amount: i128) {
    caller.require_auth();
    let mut history = get_recurring_history(e.clone(), recurring_id);
    history.push_back(RecurringPayment {
        recurring_id,
        execution_ledger: e.ledger().sequence(),
        amount,
    });
    e.storage()
        .persistent()
        .set(&DataKey::RecurringHistory(recurring_id), &history);
}

pub fn get_recurring_history(e: Env, recurring_id: u32) -> Vec<RecurringPayment> {
    e.storage()
        .persistent()
        .get(&DataKey::RecurringHistory(recurring_id))
        .unwrap_or(Vec::new(&e))
}

pub fn amend_recurring(
    e: &Env,
    caller: &Address,
    recurring_id: u32,
    new_amount: i128,
    new_interval: u32,
) {
    caller.require_auth();
    assert!(new_amount > 0, "amount must be positive");
    assert!(new_interval > 0, "interval must be positive");
    let mut record: RecurringRecord = e
        .storage()
        .persistent()
        .get(&DataKey::Recurring(recurring_id))
        .expect("recurring not found");
    assert!(record.payer == *caller, "not the payer");
    assert!(record.active, "recurring is not active");
    record.amount = new_amount;
    record.interval = new_interval;
    e.storage()
        .persistent()
        .set(&DataKey::Recurring(recurring_id), &record);
}

pub fn recurring_count_for_payee(e: Env, payee: Address) -> u32 {
    let list: soroban_sdk::Vec<u32> = e
        .storage()
        .persistent()
        .get(&DataKey::PayeeRecurrings(payee))
        .unwrap_or(soroban_sdk::Vec::new(&e));
    list.len()
}

pub fn recurring_ids_for_payee(e: Env, payee: Address) -> soroban_sdk::Vec<u32> {
    e.storage()
        .persistent()
        .get(&DataKey::PayeeRecurrings(payee))
        .unwrap_or(soroban_sdk::Vec::new(&e))
}

/// #749: admin-only global setting for how many ledgers past the due date an
/// execution is still allowed. A window of 0 disables the limit entirely.
pub fn set_recurring_execution_window(e: &Env, admin: &Address, window_ledgers: u32) {
    crate::admin::check_admin(e, admin);
    e.storage()
        .persistent()
        .set(&DataKey::RecurringExecutionWindow, &window_ledgers);
}

pub fn cancel_recurring_batch(e: &Env, caller: &Address, recurring_ids: Vec<u32>) {
    caller.require_auth();
    assert!(recurring_ids.len() <= 20, "batch size cannot exceed 20");
    for i in 0..recurring_ids.len() {
        if let Some(id) = recurring_ids.get(i) {
            let mut record: RecurringRecord = e
                .storage()
                .persistent()
                .get(&DataKey::Recurring(id))
                .expect("recurring not found");
            assert!(
                record.payer == *caller,
                "not the payer for recurring {}",
                id
            );
            assert!(record.active, "recurring {} is not active", id);
            record.active = false;
            e.storage()
                .persistent()
                .set(&DataKey::Recurring(id), &record);
        }
    }
}

pub fn cancel_recurring(e: &Env, caller: &Address, recurring_id: u32) {
    caller.require_auth();
    let mut record: RecurringRecord = e
        .storage()
        .persistent()
        .get(&DataKey::Recurring(recurring_id))
        .expect("recurring not found");
    assert!(record.payer == *caller, "not the payer");
    assert!(record.active, "recurring is not active");
    record.active = false;
    e.storage()
        .persistent()
        .set(&DataKey::Recurring(recurring_id), &record);

    let index_key = DataKey::PayerRecurrings(record.payer.clone());
    if let Some(ids) = e.storage().persistent().get::<_, Vec<u32>>(&index_key) {
        let mut updated: Vec<u32> = Vec::new(e);
        for i in 0..ids.len() {
            let v = ids.get(i).unwrap();
            if v != recurring_id {
                updated.push_back(v);
            }
        }
        e.storage().persistent().set(&index_key, &updated);
    }

    // #732: emit a cancel event for off-chain indexers
    e.events().publish(
        (soroban_sdk::symbol_short!("rcr_cnl"), record.payer.clone()),
        recurring_id,
    );
}

pub fn pause_recurring(e: &Env, caller: &Address, recurring_id: u32) {
    caller.require_auth();
    let mut record: RecurringRecord = e
        .storage()
        .persistent()
        .get(&DataKey::Recurring(recurring_id))
        .expect("recurring not found");
    // #586: verify the caller is the payer
    if record.payer != *caller {
        panic!("unauthorized: only the payer can pause a recurring payment");
    }
    assert!(record.active, "recurring is not active");
    record.active = false;
    e.storage().persistent().set(&DataKey::Recurring(recurring_id), &record);
}

pub fn resume_recurring(e: &Env, caller: &Address, recurring_id: u32) {
    caller.require_auth();
    let mut record: RecurringRecord = e
        .storage()
        .persistent()
        .get(&DataKey::Recurring(recurring_id))
        .expect("recurring not found");
    // #586: verify the caller is the payer
    if record.payer != *caller {
        panic!("unauthorized: only the payer can resume a recurring payment");
    }
    assert!(!record.active, "recurring is already active");
    record.active = true;
    e.storage().persistent().set(&DataKey::Recurring(recurring_id), &record);
}

pub fn get_recurring_by_payer(e: &Env, payer: &Address) -> Vec<u32> {
    e.storage()
        .persistent()
        .get(&DataKey::PayerRecurrings(payer.clone()))
        .unwrap_or(Vec::new(e))
}

/// #735: transfer a recurring payment to a new payer. Both the current payer
/// and the new payer must authenticate, and the payer index is updated so the
/// recurring id shows up under the new payer.
pub fn transfer_recurring_payer(e: &Env, caller: &Address, recurring_id: u32, new_payer: Address) {
    caller.require_auth();

    let mut record: RecurringRecord = e
        .storage()
        .persistent()
        .get(&DataKey::Recurring(recurring_id))
        .expect("recurring not found");
    assert!(record.payer == *caller, "not the payer");
    assert!(record.active, "recurring is not active");
    // Reject a no-op transfer before authenticating the new payer (the host
    // rejects double-auth of the same address, so this must be checked first).
    assert!(
        record.payer != new_payer,
        "new payer must differ from current payer"
    );
    new_payer.require_auth();

    // Remove the id from the old payer's index.
    let old_index = DataKey::PayerRecurrings(record.payer.clone());
    if let Some(ids) = e.storage().persistent().get::<_, Vec<u32>>(&old_index) {
        let mut updated: Vec<u32> = Vec::new(e);
        for i in 0..ids.len() {
            let v = ids.get(i).unwrap();
            if v != recurring_id {
                updated.push_back(v);
            }
        }
        e.storage().persistent().set(&old_index, &updated);
    }

    record.payer = new_payer.clone();
    e.storage()
        .persistent()
        .set(&DataKey::Recurring(recurring_id), &record);

    // Add the id to the new payer's index.
    let mut payer_ids: Vec<u32> = e
        .storage()
        .persistent()
        .get(&DataKey::PayerRecurrings(new_payer.clone()))
        .unwrap_or(Vec::new(e));
    payer_ids.push_back(recurring_id);
    e.storage()
        .persistent()
        .set(&DataKey::PayerRecurrings(new_payer), &payer_ids);

    e.events().publish(
        (soroban_sdk::symbol_short!("rcr_pyr"), caller.clone(), record.payer.clone()),
        recurring_id,
    );
}

pub fn record_execution(e: &Env, recurring_id: u32, amount: i128) {
    let ledger = e.ledger().sequence();
    let execution = RecurringExecution {
        recurring_id,
        execution_ledger: ledger,
        amount,
    };

    let key = DataKey::RecurringHistory(recurring_id);
    let mut history: Vec<RecurringExecution> = e
        .storage()
        .instance()
        .get(&key)
        .unwrap_or_else(|| Vec::new(e));

    history.push_back(execution);
    e.storage().instance().set(&key, &history);
}

pub fn index_recurring_for_payee(e: &Env, payee: &Address, recurring_id: u32) {
    let key = DataKey::PayeeRecurrings(payee.clone());
    let mut recurrings: Vec<u32> = e
        .storage()
        .instance()
        .get(&key)
        .unwrap_or_else(|| Vec::new(e));

    if !recurrings.contains(recurring_id) {
        recurrings.push_back(recurring_id);
        e.storage().instance().set(&key, &recurrings);
    }
}

pub fn remove_recurring_for_payee(e: &Env, payee: &Address, recurring_id: u32) {
    let key = DataKey::PayeeRecurrings(payee.clone());
    if let Some(mut recurrings) = e.storage().instance().get::<DataKey, Vec<u32>>(&key) {
        if let Some(index) = recurrings.iter().position(|id| id == recurring_id) {
            recurrings.remove(index as u32);
            e.storage().instance().set(&key, &recurrings);
        }
    }
}