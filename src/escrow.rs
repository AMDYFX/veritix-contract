use soroban_sdk::{contracttype, token, Address, Bytes, Env, Vec};
use crate::storage_types::{DataKey, MAX_ESCROWS_PER_DEPOSITOR, MAX_MEMO_BYTES};

#[contracttype]
#[derive(Clone)]
pub struct EscrowRecord {
    pub id: u32,
    pub depositor: Address,
    pub beneficiary: Address,
    pub token: Address,
    pub amount: i128,           // original locked amount — never changes
    pub released_amount: i128,  // #174: how much has been released so far
    pub expiry_ledger: u32,
    pub released: bool,         // true only when fully released
    pub refunded: bool,
    pub memo: Bytes,            // #175: arbitrary tag — max 64 bytes
    pub liened: bool,
    pub liened_by: Address,
    pub lien_amount: i128,
    pub created_at_ledger: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct EscrowStats {
    pub total_value_locked: i128,
}

// Anti-spam configuration threshold (5 minutes cooldown window)
const ESCROW_COOLDOWN_SECONDS: u64 = 300;

// ── Storage helpers ──────────────────────────────────────────────────────────

fn read_escrow_ids(e: &Env, key: DataKey) -> Vec<u32> {
    e.storage()
        .persistent()
        .get(&key)
        .unwrap_or(Vec::new(e))
}

fn append_escrow_id(e: &Env, key: DataKey, id: u32) {
    let mut list = read_escrow_ids(e, key.clone());
    list.push_back(id);
    e.storage().persistent().set(&key, &list);
}

pub fn load_record(e: &Env, escrow_id: u32) -> EscrowRecord {
    e.storage()
        .persistent()
        .get(&DataKey::Escrow(escrow_id))
        .expect("escrow not found")
}

pub(crate) fn save_record(e: &Env, record: &EscrowRecord) {
    e.storage()
        .persistent()
        .set(&DataKey::Escrow(record.id), record);
}

fn get_admin(e: &Env) -> Address {
    e.storage()
        .persistent()
        .get(&DataKey::Admin)
        .expect("admin not set")
}

// ── Public functions ─────────────────────────────────────────────────────────

/// Internal escrow creation without auth or rate-limit checks — used by split_to_escrow
/// which batches multiple escrows under a single sender auth.
pub(crate) fn create_escrow_batch(
    e: &Env,
    depositor: &Address,
    beneficiary: &Address,
    token_addr: &Address,
    amount: i128,
    expiry_ledger: u32,
    memo: &Bytes,
) -> u32 {
    assert!(amount > 0, "amount must be greater than zero");
    assert!(
        expiry_ledger > e.ledger().sequence(),
        "expiry_ledger must be in the future"
    );

    // #175: enforce memo length limit
    if memo.len() > 64 {
        panic!("MemoTooLong: memo cannot exceed 64 bytes");
    }

    let id: u32 = e
        .storage()
        .persistent()
        .get(&DataKey::EscrowCount)
        .unwrap_or(0);

    let record = EscrowRecord {
        id,
        depositor: depositor.clone(),
        beneficiary: beneficiary.clone(),
        token: token_addr.clone(),
        amount,
        released_amount: 0,
        expiry_ledger,
        released: false,
        refunded: false,
        memo: memo.clone(),
        liened: false,
        liened_by: depositor.clone(),
        lien_amount: 0,
        created_at_ledger: e.ledger().sequence(),
    };

    save_record(e, &record);
    append_escrow_id(e, DataKey::DepositorEscrows(depositor.clone()), id);
    append_escrow_id(e, DataKey::BeneficiaryEscrows(beneficiary.clone()), id);

    e.storage().persistent().set(&DataKey::EscrowCount, &(id + 1));
    let current_locked: i128 = e.storage().persistent().get(&DataKey::EscrowValueLocked).unwrap_or(0);
    e.storage().persistent().set(&DataKey::EscrowValueLocked, &(current_locked + amount));

    e.events().publish(
        (
            soroban_sdk::symbol_short!("escrow_cr"),
            record.depositor.clone(),
            record.beneficiary.clone(),
        ),
        (record.amount, record.memo.clone()),
    );

    id
}

/// Public escrow creation with auth and rate-limit checks.
/// #175 enforces `memo: Bytes` — max 64 bytes.
/// #269 enforces dynamic rate limiting based on block timestamp history.
pub fn create_escrow(
    e: Env,
    depositor: Address,
    beneficiary: Address,
    token_addr: Address,
    amount: i128,
    expiry_ledger: u32,
    memo: Bytes,
) -> u32 {
    depositor.require_auth();

    // #269: Strict Anti-Spam Rate Limiting Guard Check
    let rate_limit_key = DataKey::LastEscrowTime(depositor.clone());
    let last_creation_time: u64 = e.storage().persistent().get(&rate_limit_key).unwrap_or(0);
    let current_time = e.ledger().timestamp();

    if last_creation_time > 0 && (current_time - last_creation_time) < ESCROW_COOLDOWN_SECONDS {
        panic!("RateLimitExceeded: please wait before creating another escrow");
    }

    // #175: enforce memo length limit with the exact panic string required
    if memo.len() > MAX_MEMO_BYTES {
        panic!("MemoTooLong: memo cannot exceed {} bytes", MAX_MEMO_BYTES);
    }

    assert!(amount > 0, "amount must be greater than zero");
    if amount < crate::storage_types::MIN_ESCROW_AMOUNT {
        panic!("AmountTooSmall: escrow amount must be at least {} tokens", crate::storage_types::MIN_ESCROW_AMOUNT);
    }
    // #433: expiry must be strictly in the future
    assert!(
        expiry_ledger > e.ledger().sequence(),
        "expiry_ledger must be in the future"
    );

    // Closes #570: per-depositor escrow count limit
    let depositor_escrows: Vec<u32> = e
        .storage()
        .persistent()
        .get(&DataKey::DepositorEscrows(depositor.clone()))
        .unwrap_or(Vec::new(&e));
    if depositor_escrows.len() >= MAX_ESCROWS_PER_DEPOSITOR {
        panic!("TooManyEscrows: depositor has reached the active escrow limit");
    }

    let id: u32 = e
        .storage()
        .persistent()
        .get(&DataKey::EscrowCount)
        .unwrap_or(0);

    // Pull tokens from depositor into the contract
    let token_client = token::Client::new(&e, &token_addr);
    token_client.transfer(&depositor, &e.current_contract_address(), &amount);

    let record = EscrowRecord {
        id,
        depositor: depositor.clone(),
        beneficiary: beneficiary.clone(),
        token: token_addr,
        amount,
        released_amount: 0, // #174: starts at zero
        expiry_ledger,
        released: false,
        refunded: false,
        memo,               // #175
        liened: false,
        liened_by: depositor.clone(),
        lien_amount: 0,
        created_at_ledger: e.ledger().sequence(),
    };

    save_record(&e, &record);
    append_escrow_id(&e, DataKey::DepositorEscrows(depositor.clone()), id);
    append_escrow_id(&e, DataKey::BeneficiaryEscrows(beneficiary), id);

    // Update state tracking counters and rate limit timestamps cleanly
    e.storage().persistent().set(&DataKey::EscrowCount, &(id + 1));
    // Add the new escrow amount to the total value locked counter
    let current_locked: i128 = e.storage().persistent().get(&DataKey::EscrowValueLocked).unwrap_or(0);
    e.storage().persistent().set(&DataKey::EscrowValueLocked, &(current_locked + amount));
    e.storage().persistent().set(&rate_limit_key, &current_time);

    // #181: emit escrow_created event with memo for indexers
    e.events().publish(
        (
            soroban_sdk::symbol_short!("escrow_cr"),
            record.depositor.clone(),
            record.beneficiary.clone(),
        ),
        (record.amount, record.memo.clone()),
    );

    id
}

/// Full release — sends everything remaining to the beneficiary.
pub fn release_escrow(e: Env, caller: Address, escrow_id: u32) {
    caller.require_auth();

    let mut record = load_record(&e, escrow_id);

    assert!(!record.released, "already released");
    assert!(!record.refunded, "already refunded");
    assert!(
        caller == record.depositor || caller == get_admin(&e),
        "not authorised to release"
    );
    assert!(
        e.ledger().sequence() <= record.expiry_ledger,
        "escrow has expired"
    );

    let remaining = record.amount - record.released_amount;
    assert!(remaining > 0, "nothing left to release");

    record.released_amount = record.amount;
    record.released = true;
    
    let token_client = token::Client::new(&e, &record.token);
    
        let lien_transfer = if record.liened && record.lien_amount > 0 {
        let l_amount = record.lien_amount;
        let l_by = record.liened_by.clone();
        // Clear the lien
        record.liened = false;
        record.lien_amount = 0;
        
        let to_lien = core::cmp::min(l_amount, remaining);
        token_client.transfer(&e.current_contract_address(), &l_by, &to_lien);
        to_lien
    } else {
        0
    };
    
    save_record(&e, &record);

    let mut to_beneficiary = remaining - lien_transfer;

    // #454: Protocol fee deduction
    let fee_bps: u32 = e.storage().persistent().get(&DataKey::FeeBps).unwrap_or(0);
    let fee_amount = if fee_bps > 0 && to_beneficiary > 0 {
        let f = to_beneficiary * fee_bps as i128 / 10_000;
        if f > 0 {
            let treasury: Address = e
                .storage()
                .persistent()
                .get(&DataKey::TreasuryAddress)
                .expect("treasury not set");
            token_client.transfer(&e.current_contract_address(), &treasury, &f);
            let prev: i128 = e
                .storage()
                .persistent()
                .get(&DataKey::TotalFeesCollected)
                .unwrap_or(0);
            e.storage()
                .persistent()
                .set(&DataKey::TotalFeesCollected, &(prev + f));
            f
        } else {
            0
        }
    } else {
        0
    };
    to_beneficiary -= fee_amount;

    if to_beneficiary > 0 {
        token_client.transfer(&e.current_contract_address(), &record.beneficiary, &to_beneficiary);
    }

    // Subtract the released amount from the total value locked counter
    let current_locked: i128 = e.storage().persistent().get(&DataKey::EscrowValueLocked).unwrap_or(0);
    e.storage().persistent().set(&DataKey::EscrowValueLocked, &(current_locked - remaining));

    // #181: emit escrow_released event with memo for indexers
    e.events().publish(
        (
            soroban_sdk::symbol_short!("escrow_rl"),
            record.depositor.clone(),
            record.beneficiary.clone(),
        ),
        (remaining, record.memo.clone()),
    );
}

/// #174: Partial release — caller must be the beneficiary.
pub fn release_partial_escrow(e: Env, caller: Address, escrow_id: u32, amount: i128) {
    caller.require_auth();

    let mut record = load_record(&e, escrow_id);

    assert!(!record.refunded, "already refunded");
    assert!(!record.released, "already fully released");
    assert!(
        caller == record.beneficiary,
        "only the beneficiary can partially release"
    );
    assert!(
        e.ledger().sequence() <= record.expiry_ledger,
        "escrow has expired"
    );
    assert!(amount > 0, "release amount must be greater than zero");

    let remaining = record.amount - record.released_amount;
    assert!(
        amount <= remaining,
        "release amount exceeds remaining balance"
    );

    record.released_amount += amount;

    // Mark fully released if nothing is left
    if record.released_amount == record.amount {
        record.released = true;
    }

    save_record(&e, &record);

    let token_client = token::Client::new(&e, &record.token);
    token_client.transfer(&e.current_contract_address(), &record.beneficiary, &amount);

    // Subtract the partially released amount from the total value locked counter
    let current_locked: i128 = e.storage().persistent().get(&DataKey::EscrowValueLocked).unwrap_or(0);
    e.storage().persistent().set(&DataKey::EscrowValueLocked, &(current_locked - amount));
}

/// Refund — returns original locked amount minus what was already partially released.
pub fn refund_escrow(e: Env, caller: Address, escrow_id: u32) {
    caller.require_auth();

    let mut record = load_record(&e, escrow_id);

    let is_disputed: bool = e.storage().persistent().get(&DataKey::EscrowDispute(escrow_id)).unwrap_or(false);
    assert!(!is_disputed, "cannot refund active dispute");

    assert!(!record.released, "already released");
    assert!(!record.refunded, "already refunded");
    assert!(
        caller == record.depositor || caller == get_admin(&e),
        "not authorised to refund"
    );

    let refundable = record.amount - record.released_amount;
    assert!(refundable > 0, "nothing left to refund");

    record.refunded = true;
    save_record(&e, &record);

    let token_client = token::Client::new(&e, &record.token);
    token_client.transfer(
        &e.current_contract_address(),
        &record.depositor,
        &refundable,
    );

    // Subtract the refunded amount from the total value locked counter
    let current_locked: i128 = e.storage().persistent().get(&DataKey::EscrowValueLocked).unwrap_or(0);
    e.storage().persistent().set(&DataKey::EscrowValueLocked, &(current_locked - refundable));

    // #181: emit escrow_refunded event with memo for indexers
    e.events().publish(
        (
            soroban_sdk::symbol_short!("escrow_rf"),
            record.depositor.clone(),
            record.beneficiary.clone(),
        ),
        (refundable, record.memo.clone()),
    );
}

pub fn place_lien(e: Env, creditor: Address, escrow_id: u32, lien_amount: i128) {
    creditor.require_auth();
    let mut record = load_record(&e, escrow_id);
    
    assert!(!record.released && !record.refunded, "escrow already settled");
    assert!(!record.liened, "only one lien at a time");
    assert!(lien_amount > 0, "lien amount must be positive");
    assert!(lien_amount <= record.amount, "lien amount exceeds escrow amount");
    
    record.liened = true;
    record.liened_by = creditor;
    record.lien_amount = lien_amount;
    save_record(&e, &record);
}

pub fn clear_lien(e: Env, caller: Address, escrow_id: u32) {
    caller.require_auth();
    let mut record = load_record(&e, escrow_id);
    
    assert!(!record.released && !record.refunded, "escrow already settled");
    assert!(record.liened, "no active lien");
    
    let lien_owner = record.liened_by.clone();
    assert!(caller == record.depositor || caller == lien_owner, "not authorized to clear lien");
    
    record.liened = false;
    record.lien_amount = 0;
    save_record(&e, &record);
}

// ── Query helpers ─────────────────────────────────────────────────────────────

pub fn get_escrows_by_depositor(e: Env, depositor: Address) -> Vec<u32> {
    read_escrow_ids(&e, DataKey::DepositorEscrows(depositor))
}

pub fn get_escrows_by_beneficiary(e: Env, beneficiary: Address) -> Vec<u32> {
    read_escrow_ids(&e, DataKey::BeneficiaryEscrows(beneficiary))
}

pub fn get_escrow_stats(e: &Env) -> EscrowStats {
    let total_value_locked: i128 = e.storage().persistent().get(&DataKey::EscrowValueLocked).unwrap_or(0);
    EscrowStats {
        total_value_locked,
    }
}

pub fn get_escrowed_total(e: &Env) -> i128 {
    // Maintain backwards compatibility
    let stats = get_escrow_stats(e);
    stats.total_value_locked
}

pub fn get_escrows_batch(e: Env, escrow_ids: Vec<u32>) -> Vec<Option<EscrowRecord>> {
    assert!(escrow_ids.len() <= 50, "batch size cannot exceed 50");
    let mut result = Vec::new(&e);
    for id in escrow_ids {
        let record = e.storage().persistent().get::<_, EscrowRecord>(&DataKey::Escrow(id));
        result.push_back(record);
    }
    result
}

pub fn get_escrow_age(e: Env, escrow_id: u32) -> u32 {
    let record = load_record(&e, escrow_id);
    if record.released || record.refunded {
        0
    } else {
        e.ledger().sequence().saturating_sub(record.created_at_ledger)
    }
}

pub fn topup_escrow(e: Env, depositor: Address, escrow_id: u32, amount: i128) {
    depositor.require_auth();
    assert!(amount > 0, "amount must be positive");
    if e.storage().persistent().has(&DataKey::EscrowDispute(escrow_id)) {
        panic!("DisputeOpen: cannot top up an escrow under active dispute");
    }
    let mut record = load_record(&e, escrow_id);
    assert!(!record.released && !record.refunded, "escrow already settled");
    assert!(record.depositor == depositor, "not the depositor");
    let token_client = token::Client::new(&e, &record.token);
    token_client.transfer(&depositor, &e.current_contract_address(), &amount);
    record.amount += amount;
    save_record(&e, &record);
}

// ── #452: Escrowed value for a specific depositor ─────────────────────────────

pub fn escrowed_value_for_depositor(e: &Env, depositor: &Address) -> i128 {
    let escrow_ids: Vec<u32> = e
        .storage()
        .persistent()
        .get(&DataKey::DepositorEscrows(depositor.clone()))
        .unwrap_or(Vec::new(e));

    let mut total = 0_i128;
    for i in 0..escrow_ids.len() {
        let id = escrow_ids.get(i).unwrap();
        let record: EscrowRecord = e
            .storage()
            .persistent()
            .get(&DataKey::Escrow(id))
            .unwrap_or_else(|| panic!("escrow {} not found", id));

        if !record.released && !record.refunded {
            total += record.amount - record.released_amount;
        }
    }

    total
}

pub fn trigger_auto_release(e: Env, escrow_id: u32) {
    let release_ledger: u32 = e.storage().persistent()
        .get(&DataKey::AutoRelease(escrow_id))
        .expect("auto release not set for this escrow");
    assert!(e.ledger().sequence() >= release_ledger, "auto release not yet available");
    let record = load_record(&e, escrow_id);
    assert!(!record.released && !record.refunded, "escrow already settled");
    release_escrow(e, record.depositor, escrow_id)
}

pub fn escrow_between(e: Env, addr1: Address, addr2: Address) -> u32 {
    let dep_escrows = get_escrows_by_depositor(e.clone(), addr1.clone());
    let ben_escrows = get_escrows_by_beneficiary(e.clone(), addr2.clone());
    for i in 0..dep_escrows.len() {
        if let Some(id) = dep_escrows.get(i) {
            for j in 0..ben_escrows.len() {
                if let Some(ben_id) = ben_escrows.get(j) {
                    if id == ben_id { return id; }
                }
            }
        }
    }
    let dep_escrows2 = get_escrows_by_depositor(e.clone(), addr2);
    let ben_escrows2 = get_escrows_by_beneficiary(e, addr1);
    for i in 0..dep_escrows2.len() {
        if let Some(id) = dep_escrows2.get(i) {
            for j in 0..ben_escrows2.len() {
                if let Some(ben_id) = ben_escrows2.get(j) {
                    if id == ben_id { return id; }
                }
            }
        }
    }
    panic!("no escrow found between the two addresses");
}
