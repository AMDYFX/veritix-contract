use crate::events::{EscrowCreated, EscrowRefunded, EscrowReleased};
use crate::storage_types::{DataKey, EscrowRecord, EscrowStatus};
use crate::validation::require_positive_amount;
use soroban_sdk::{token, Address, Env};

/// Number of escrows created so far, which is also the next escrow id.
///
/// Ids are handed out by incrementing this counter rather than by deriving
/// anything from the call, so an id is never reused and a settled escrow can
/// never be confused with a live one that happens to sit at the same storage
/// offset.
pub fn count(e: &Env) -> u32 {
    e.storage()
        .persistent()
        .get(&DataKey::EscrowCount)
        .unwrap_or(0)
}

/// Total token amount still held across all active escrows for a given token.
pub fn value_locked(e: &Env, token: &Address) -> i128 {
    e.storage()
        .persistent()
        .get(&DataKey::EscrowValueLocked(token.clone()))
        .unwrap_or(0)
}

/// Stores an escrow record under the next id and advances the counter past it.
fn store_new(e: &Env, record: &EscrowRecord) -> u32 {
    let id = count(e);
    e.storage()
        .persistent()
        .set(&DataKey::EscrowRecord(id), record);
    let next_id = id
        .checked_add(1)
        .unwrap_or_else(|| panic!("EscrowIdOverflow: escrow id would overflow u32"));
    e.storage()
        .persistent()
        .set(&DataKey::EscrowCount, &next_id);
    index_push(e, DataKey::DepositorEscrows(record.depositor.clone()), id);
    index_push(
        e,
        DataKey::BeneficiaryEscrows(record.beneficiary.clone()),
        id,
    );
    id
}

/// Largest page returned by [`ids_page`].
pub const MAX_PAGE: u32 = 50;

fn index_ids(e: &Env, key: &DataKey) -> soroban_sdk::Vec<u32> {
    e.storage()
        .persistent()
        .get(key)
        .unwrap_or_else(|| soroban_sdk::Vec::new(e))
}

fn index_push(e: &Env, key: DataKey, id: u32) {
    let mut ids = index_ids(e, &key);
    ids.push_back(id);
    e.storage().persistent().set(&key, &ids);
}

fn index_remove(e: &Env, key: DataKey, id: u32) {
    let mut ids = index_ids(e, &key);
    if let Some(pos) = ids.first_index_of(id) {
        ids.remove(pos);
    }
    e.storage().persistent().set(&key, &ids);
}

/// A page of the escrow ids indexed under `key`, starting at `start` and
/// holding at most `limit` (capped at [`MAX_PAGE`]) ids.
pub fn ids_page(e: &Env, key: &DataKey, start: u32, limit: u32) -> soroban_sdk::Vec<u32> {
    let all = index_ids(e, key);
    let limit = if limit > MAX_PAGE { MAX_PAGE } else { limit };
    let end = core::cmp::min(start.saturating_add(limit), all.len());
    let mut out = soroban_sdk::Vec::new(e);
    let mut i = start;
    while i < end {
        if let Some(id) = all.get(i) {
            out.push_back(id);
        }
        i += 1;
    }
    out
}

/// Total token amount still held across all active escrows for a given token.
pub fn escrowed_total(e: &Env, token: &Address) -> i128 {
    value_locked(e, token)
}

/// `(total locked for token, active count, settled count)` over every escrow created for `token`.
///
/// Walks every escrow, so its cost grows with the number of escrows.
pub fn stats(e: &Env, token: &Address) -> (i128, u32, u32) {
    let mut active = 0u32;
    let mut settled = 0u32;
    let mut id = 0u32;
    let n = count(e);
    while id < n {
        let rec = record(e, id);
        if &rec.token == token {
            if is_settled_status(&rec.status) {
                settled += 1;
            } else {
                active += 1;
            }
        }
        id += 1;
    }
    (value_locked(e, token), active, settled)
}

/// Overwrites the record stored at `id`.
fn store_record(e: &Env, id: u32, record: &EscrowRecord) {
    e.storage()
        .persistent()
        .set(&DataKey::EscrowRecord(id), record);
}

/// Adds `amount` to the total held in active escrows for `token`.
fn increase_value_locked(e: &Env, token: &Address, amount: i128) {
    let new_total = value_locked(e, token)
        .checked_add(amount)
        .unwrap_or_else(|| panic!("EscrowOverflow: locked value would overflow i128"));
    e.storage()
        .persistent()
        .set(&DataKey::EscrowValueLocked(token.clone()), &new_total);
}

/// Takes `amount` out of the total held in active escrows for `token`, refusing to go
/// negative.
///
/// A settlement that could drive the total below zero would mean the contract
/// paid out more than it was holding, and every caller that could reach it is
/// one a user has already trusted with the escrow. The subtraction is checked
/// rather than saturating so a bug surfaces here instead of as a silently wrong
/// total.
pub(crate) fn decrease_value_locked(e: &Env, token: &Address, amount: i128) {
    let current = value_locked(e, token);
    let new_total = current.checked_sub(amount).unwrap_or_else(|| {
        panic!(
            "EscrowUnderflow: paying out {} would exceed the {} still held",
            amount, current
        )
    });
    e.storage()
        .persistent()
        .set(&DataKey::EscrowValueLocked(token.clone()), &new_total);
}

/// Reads an escrow record.
///
/// # Panics
///
/// Panics with `EscrowNotFound` when no escrow exists at `id`. A client asking
/// about an escrow that was never created should hear that, rather than a
/// plausible-looking default.
pub fn record(e: &Env, id: u32) -> EscrowRecord {
    e.storage()
        .persistent()
        .get(&DataKey::EscrowRecord(id))
        .unwrap_or_else(|| panic!("EscrowNotFound: no escrow with id {}", id))
}

/// Maximum number of ids accepted by [`records_batch`].
pub const MAX_BATCH: u32 = 50;

/// Ledgers elapsed since escrow `id` was created.
///
/// # Panics
///
/// Panics with `EscrowNotFound` when no escrow exists at `id`.
pub fn age(e: &Env, id: u32) -> u32 {
    e.ledger()
        .sequence()
        .saturating_sub(record(e, id).created_ledger)
}

/// Reads many escrows at once, preserving order and returning `None` for ids
/// that do not exist.
///
/// # Panics
///
/// Panics with `EscrowBatchTooLarge` when more than [`MAX_BATCH`] ids are given.
pub fn records_batch(
    e: &Env,
    ids: &soroban_sdk::Vec<u32>,
) -> soroban_sdk::Vec<Option<EscrowRecord>> {
    if ids.len() > MAX_BATCH {
        panic!(
            "EscrowBatchTooLarge: {} ids requested, at most {} allowed",
            ids.len(),
            MAX_BATCH
        );
    }
    let mut out = soroban_sdk::Vec::new(e);
    for id in ids.iter() {
        out.push_back(e.storage().persistent().get(&DataKey::EscrowRecord(id)));
    }
    out
}

/// Whether `status` is terminal.
pub fn is_settled_status(status: &EscrowStatus) -> bool {
    matches!(status, EscrowStatus::Released | EscrowStatus::Refunded)
}

/// Whether the escrow at `id` has been settled.
///
/// # Panics
///
/// Panics with `EscrowNotFound` when no escrow exists at `id`, for the same
/// reason [`record`] does: "not settled" would be a false answer to a question
/// about an escrow that does not exist, and a client would carry that forward.
pub fn is_settled(e: &Env, id: u32) -> bool {
    is_settled_status(&record(e, id).status)
}

/// Panics while an open dispute freezes the escrow.
///
/// A buyer who did not get into the event must be able to stop the organizer
/// being paid: once `raise_dispute` opens a dispute over the escrow, neither
/// release nor refund may move funds until a ruling is final.
fn require_no_open_dispute(e: &Env, id: u32) {
    if crate::dispute::is_dispute_open(e, id as u64) {
        panic!("EscrowDisputed: escrow {} is frozen by an open dispute", id);
    }
}

/// Panics unless `record` is still `Active`.
fn require_active(id: u32, record: &EscrowRecord) {
    if record.status == EscrowStatus::Disputed {
        panic!("EscrowDisputed: escrow {} is frozen by an open dispute", id);
    }
    if is_settled_status(&record.status) {
        panic!(
            "EscrowNotActive: escrow {} is already {:?}",
            id, record.status
        );
    }
}

/// Requires that `caller` may settle `record`.
///
/// Either the depositor — who paid the money and is entitled to take it back —
/// or the contract admin, who is the backstop when the two parties cannot
/// agree. The check comes before the state is read as final and before any
/// transfer, so an unauthorized caller cannot move a token.
fn require_settlement_authority(e: &Env, caller: &Address, record: &EscrowRecord) {
    if &record.depositor != caller && !crate::admin::is_admin(e, caller) {
        panic!(
            "Unauthorized: only the depositor or the admin may settle escrow for {:?}",
            record.beneficiary
        );
    }
    caller.require_auth();
}

/// Requires that `caller` is authorized to refund `record`.
///
/// The admin or beneficiary may refund at any time; the depositor may only
/// refund after the deadline ledger has passed.
fn require_refund_authority(e: &Env, caller: &Address, record: &EscrowRecord) {
    let is_admin = crate::admin::is_admin(e, caller);
    let is_beneficiary = &record.beneficiary == caller;
    let is_depositor = &record.depositor == caller;

    if !is_admin && !is_beneficiary && !is_depositor {
        panic!("Unauthorized: only the admin, beneficiary, or depositor may refund escrow");
    }

    if is_depositor
        && !is_admin
        && !is_beneficiary
        && e.ledger().sequence() <= record.deadline_ledger
    {
        panic!(
            "EscrowDeadlineNotPassed: depositor may only refund after deadline ledger {}",
            record.deadline_ledger
        );
    }

    caller.require_auth();
}

/// Sends `amount` of `token` out of the contract to `to`.
fn pay(e: &Env, token_address: &Address, to: &Address, amount: i128) {
    token::TokenClient::new(e, token_address).transfer(&e.current_contract_address(), to, &amount);
}

/// Holds `amount` of `token` for `beneficiary` and returns the new escrow's id.
///
/// Funds leave the depositor's account immediately and only reach the
/// beneficiary once the event completes, so the contract — not the depositor —
/// holds them in the meantime.
///
/// The transfer goes through the token's own SEP-41 client rather than this
/// contract's internal ledger, because the escrowed token is a separate
/// contract: a caller may escrow a token this contract knows nothing about.
///
/// # Panics
///
/// Panics through [`require_positive_amount`] on a non-positive `amount`, with
/// `InvalidDeadline` when `deadline_ledger <= e.ledger().sequence()`, and
/// propagates whatever the token contract raises when the depositor's balance
/// or the token's own authorization rules are not satisfied.
pub fn create(
    e: &Env,
    depositor: &Address,
    beneficiary: &Address,
    token_address: &Address,
    amount: i128,
    deadline_ledger: u32,
) -> u32 {
    require_positive_amount(amount);
    if deadline_ledger <= e.ledger().sequence() {
        panic!(
            "InvalidDeadline: deadline ledger {} must be strictly greater than current ledger {}",
            deadline_ledger,
            e.ledger().sequence()
        );
    }
    if depositor == beneficiary {
        panic!("EscrowSameParty: depositor and beneficiary must be different addresses");
    }
    depositor.require_auth();

    // Move the funds before recording the escrow. A token transfer that fails
    // panics and reverts the whole invocation, so there is no window in which
    // an escrow exists for funds that never arrived.
    let contract_address = e.current_contract_address();
    token::TokenClient::new(e, token_address).transfer(depositor, &contract_address, &amount);
    let id = store_new(
        e,
        &EscrowRecord {
            depositor: depositor.clone(),
            beneficiary: beneficiary.clone(),
            token: token_address.clone(),
            amount,
            deadline_ledger,
            status: EscrowStatus::Active,
            created_ledger: e.ledger().sequence(),
        },
    );
    increase_value_locked(e, token_address, amount);

    EscrowCreated {
        depositor: depositor.clone(),
        beneficiary: beneficiary.clone(),
        id,
        token: token_address.clone(),
        amount,
        deadline_ledger,
    }
    .publish(e);

    id
}

/// Pays the beneficiary everything still held and closes the escrow as
/// `Released`.
pub fn release(e: &Env, caller: &Address, id: u32) {
    require_no_open_dispute(e, id);
    let mut record = record(e, id);
    require_active(id, &record);
    require_settlement_authority(e, caller, &record);

    let amount = record.amount;
    let fee = crate::fee::calculate_fee(e, amount);
    let payout = amount.checked_sub(fee).unwrap_or(amount);

    if fee > 0 {
        if let Some(treasury) = crate::fee::get_treasury(e) {
            pay(e, &record.token, &treasury, fee);
        }
    }
    pay(e, &record.token, &record.beneficiary, payout);
    decrease_value_locked(e, &record.token, amount);
    record.status = EscrowStatus::Released;
    record.amount = 0;
    store_record(e, id, &record);

    EscrowReleased {
        beneficiary: record.beneficiary.clone(),
        id,
        amount,
        remaining: 0,
    }
    .publish(e);
}

/// Returns everything still held to the depositor and closes the escrow as
/// `Refunded`.
///
/// # Panics
///
/// Panics with `EscrowNotActive` when the escrow has already been settled, so a
/// refund can never race a release into paying the same funds twice.
pub fn refund(e: &Env, caller: &Address, id: u32) {
    require_no_open_dispute(e, id);
    let mut record = record(e, id);
    require_active(id, &record);
    require_refund_authority(e, caller, &record);

    let amount = record.amount;
    pay(e, &record.token, &record.depositor, amount);
    decrease_value_locked(e, &record.token, amount);
    record.status = EscrowStatus::Refunded;
    record.amount = 0;
    store_record(e, id, &record);

    EscrowRefunded {
        depositor: record.depositor.clone(),
        id,
        amount,
    }
    .publish(e);
}

/// Emitted when the admin settles an escrow by hand.
#[soroban_sdk::contractevent(data_format = "vec")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowAdminSettled {
    #[topic]
    pub admin: Address,
    #[topic]
    pub winner: Address,
    pub id: u32,
    pub amount: i128,
}

/// Releases an escrow to its beneficiary once its deadline ledger has passed.
///
/// Callable by anyone. Refuses while a dispute is open.
///
/// # Panics
///
/// Panics with `EscrowNotExpired` when the current ledger is not past the
/// deadline, and with `EscrowNotActive` or `EscrowDisputed` as for [`release`].
pub fn auto_release(e: &Env, id: u32) {
    require_no_open_dispute(e, id);
    let mut record = record(e, id);
    require_active(id, &record);
    if e.ledger().sequence() <= record.deadline_ledger {
        panic!(
            "EscrowNotExpired: escrow {} deadline ledger {} has not passed",
            id, record.deadline_ledger
        );
    }

    let amount = record.amount;
    pay(e, &record.token, &record.beneficiary, amount);
    decrease_value_locked(e, &record.token, amount);
    record.status = EscrowStatus::Released;
    record.amount = 0;
    store_record(e, id, &record);

    EscrowReleased {
        beneficiary: record.beneficiary.clone(),
        id,
        amount,
        remaining: 0,
    }
    .publish(e);
}

/// Lets the admin settle an unsettled escrow, even one stuck behind an
/// abandoned dispute, by paying the remaining amount to `winner`.
///
/// # Panics
///
/// Panics when `admin` is not the admin, with `EscrowNotActive` when the escrow
/// is already settled, and with `WinnerNotEscrowParty` when `winner` is neither
/// the depositor nor the beneficiary.
pub fn admin_settle(e: &Env, admin: &Address, id: u32, winner: &Address) {
    crate::admin::check_admin(e, admin);
    let mut record = record(e, id);
    if is_settled_status(&record.status) {
        panic!(
            "EscrowNotActive: escrow {} is already {:?}",
            id, record.status
        );
    }
    if winner != &record.depositor && winner != &record.beneficiary {
        panic!("WinnerNotEscrowParty: winner must be the depositor or the beneficiary");
    }

    let amount = record.amount;
    pay(e, &record.token, winner, amount);
    decrease_value_locked(e, &record.token, amount);
    record.status = if winner == &record.beneficiary {
        EscrowStatus::Released
    } else {
        EscrowStatus::Refunded
    };
    record.amount = 0;
    store_record(e, id, &record);

    EscrowAdminSettled {
        admin: admin.clone(),
        winner: winner.clone(),
        id,
        amount,
    }
    .publish(e);
}

/// Pays `amount` of the held funds to the beneficiary without closing the
/// escrow, unless it was the last of the money.
///
/// A partially-delivered event is settled over several calls; the record's
/// `amount` is the remainder, and the escrow becomes `Released` on the call that
/// takes it to zero. That keeps "is anything still owed" a single field read
/// rather than a sum over the event log.
///
/// # Panics
///
/// Panics through [`require_positive_amount`] on a non-positive `amount`, with
/// `EscrowNotActive` when the escrow is already settled, with
/// `AmountAboveRemainder` when `amount` exceeds what is still held, and with
/// whatever the token contract raises on a failed transfer.
pub fn release_partial(e: &Env, caller: &Address, id: u32, amount: i128) {
    require_positive_amount(amount);
    require_no_open_dispute(e, id);
    let mut record = record(e, id);
    require_active(id, &record);
    require_settlement_authority(e, caller, &record);
    if amount > record.amount {
        panic!(
            "AmountAboveRemainder: {} requested, {} remaining",
            amount, record.amount
        );
    }

    pay(e, &record.token, &record.beneficiary, amount);
    decrease_value_locked(e, &record.token, amount);
    let remaining = record.amount - amount;
    record.amount = remaining;
    if remaining == 0 {
        record.status = EscrowStatus::Released;
    }
    store_record(e, id, &record);

    EscrowReleased {
        beneficiary: record.beneficiary.clone(),
        id,
        amount,
        remaining,
    }
    .publish(e);
}

/// Reassigns an unsettled escrow to `new_beneficiary` and keeps the beneficiary
/// index in sync.
///
/// # Panics
///
/// Panics with `Unauthorized` unless `depositor` is the escrow's depositor, and
/// with `EscrowNotActive` / `EscrowDisputed` as for [`release`].
pub fn transfer_beneficiary(e: &Env, depositor: &Address, id: u32, new_beneficiary: &Address) {
    require_no_open_dispute(e, id);
    let mut record = record(e, id);
    require_active(id, &record);
    if &record.depositor != depositor {
        panic!(
            "Unauthorized: only the depositor may change the beneficiary of escrow {}",
            id
        );
    }
    depositor.require_auth();

    index_remove(
        e,
        DataKey::BeneficiaryEscrows(record.beneficiary.clone()),
        id,
    );
    index_push(e, DataKey::BeneficiaryEscrows(new_beneficiary.clone()), id);
    record.beneficiary = new_beneficiary.clone();
    store_record(e, id, &record);
}
