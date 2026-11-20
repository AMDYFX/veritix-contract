//! Dispute resolution over escrows.
//!
//! A dispute freezes its escrow until a ruling is final. The lifecycle:
//!
//! 1. `raise_dispute` opens the dispute (`Open`) and blocks release/refund.
//! 2. `resolve_dispute` records the arbiter's ruling (`Resolved`, the winner
//!    and the ruling ledger). The payout stays **blocked**: per the appeal
//!    design, one arbiter ruling with no recourse is a poor guarantee, so the
//!    losing party has an appeal window measured in ledgers. Calling
//!    `resolve_dispute` again once the window has lapsed finalizes the ruling,
//!    settles the escrow and pays the winner.
//! 3. `appeal_dispute` — callable by the losing party inside the window —
//!    escalates the dispute (`Appealed`) and keeps the payout blocked.
//! 4. `resolve_appeal` hands down a final, unappealable ruling that settles the
//!    escrow, pays the declared winner and marks the dispute final. A second
//!    appeal on the same escrow is rejected.
//!
//! `is_dispute_open`/`get_dispute` are the read views clients use to render
//! dispute state without guessing from the escrow status.

use soroban_sdk::{Address, Env};

use crate::storage_types::{
    bump_persistent, DataKey, DisputeRecord, DisputeStatus, EscrowRecord, EscrowStatus,
};

/// How many ledgers the losing party has to appeal a ruling.
///
/// Roughly seven days at Stellar's five-second ledger close. Measured from the
/// ledger at which the ruling was recorded.
pub const APPEAL_WINDOW_LEDGERS: u32 = 120_960;

/// Reads a dispute record, extending its storage TTL.
///
/// # Panics
///
/// With `DisputeNotFound` when no dispute is open over `escrow_id`.
fn read_dispute(e: &Env, escrow_id: u32) -> DisputeRecord {
    let key = DataKey::Dispute(escrow_id);
    let record = e
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| panic!("DisputeNotFound: no dispute over escrow {}", escrow_id));
    bump_persistent(e, &key);
    record
}

/// Writes a dispute record, extending its storage TTL.
fn write_dispute(e: &Env, escrow_id: u32, record: &DisputeRecord) {
    let key = DataKey::Dispute(escrow_id);
    e.storage().persistent().set(&key, record);
    bump_persistent(e, &key);
}

/// The storage key `escrow.rs` stores escrow `escrow_id` under.
fn escrow_key(escrow_id: u32) -> DataKey {
    DataKey::EscrowRecord(escrow_id)
}

/// Reads the escrow a dispute settles, extending its storage TTL.
///
/// # Panics
///
/// With `EscrowNotFound` when no escrow `escrow_id` exists.
fn read_escrow(e: &Env, escrow_id: u32) -> EscrowRecord {
    let key = escrow_key(escrow_id);
    let record = e
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| panic!("EscrowNotFound: no escrow {}", escrow_id));
    bump_persistent(e, &key);
    record
}

/// Writes an escrow record, extending its storage TTL.
fn write_escrow(e: &Env, escrow_id: u32, record: &EscrowRecord) {
    let key = escrow_key(escrow_id);
    e.storage().persistent().set(&key, record);
    bump_persistent(e, &key);
}

/// The globally configured dispute arbiter (resolver).
///
/// Disputes need a designated resolver before any of them can be settled.
/// Set once by the admin; individual disputes may still carry their own
/// per-dispute `resolver`, which takes precedence when present.
pub fn set_arbiter(e: &Env, admin: &Address, arbiter: &Address) {
    crate::admin::check_admin(e, admin);
    e.storage().persistent().set(&DataKey::Arbiter, arbiter);
}

/// The configured arbiter.
///
/// # Panics
///
/// With `ArbiterNotSet` when no arbiter has been configured yet.
pub fn get_arbiter(e: &Env) -> Address {
    e.storage()
        .persistent()
        .get(&DataKey::Arbiter)
        .unwrap_or_else(|| panic!("ArbiterNotSet: no dispute arbiter configured"))
}

/// Raised the dispute over `escrow_id`, freezing it until a ruling is final.
///
/// The caller must be a party to the escrow (the depositor or the
/// beneficiary) and the escrow must still be `Active`. Raising stores a
/// `DisputeRecord`, marks the escrow `Disputed`, and emits a `DisputeRaised`
/// event. While the dispute is open, release and refund must panic (see
/// `is_dispute_open`).
///
/// More than one dispute over the same escrow would let a second claimant
/// double-freeze an escrow that has already been settled by a ruling, so a
/// dispute can only be raised once per escrow.
///
/// # Panics
///
/// With `DisputeAlreadyOpen` when a dispute already exists for `escrow_id`,
/// with `EscrowNotFound` for an unknown escrow, with `EscrowNotActive` unless
/// the escrow is `Active`, and with `Unauthorized` when the caller is neither
/// the depositor nor the beneficiary.
pub fn raise_dispute(e: &Env, claimant: &Address, escrow_id: u32, resolver: Option<Address>) {
    if e.storage()
        .persistent()
        .get::<_, DisputeRecord>(&DataKey::Dispute(escrow_id))
        .is_some()
    {
        panic!(
            "DisputeAlreadyOpen: escrow {} already has a dispute",
            escrow_id
        );
    }
    claimant.require_auth();
    let mut escrow = read_escrow(e, escrow_id);
    if escrow.status != EscrowStatus::Active {
        panic!(
            "EscrowNotActive: escrow {} is {:?}, not Active",
            escrow_id, escrow.status
        );
    }
    if claimant != &escrow.depositor && claimant != &escrow.beneficiary {
        panic!(
            "Unauthorized: only the depositor or the beneficiary may dispute escrow {}",
            escrow_id
        );
    }
    let resolved_resolver = match resolver {
        Some(r) => {
            if &r == claimant || r == escrow.depositor || r == escrow.beneficiary {
                panic!("DisputeSelfArbitration: a party cannot arbitrate their own dispute");
            }
            Some(r)
        }
        None => Some(get_arbiter(e)),
    };
    let record = DisputeRecord {
        claimant: claimant.clone(),
        escrow_id,
        status: DisputeStatus::Open,
        opened_ledger: e.ledger().sequence(),
        resolver: resolved_resolver,
        resolved_ledger: 0,
        winner: None,
        is_final: false,
    };
    write_dispute(e, escrow_id, &record);
    crate::dispute_stats::index_dispute(e, claimant, escrow_id);
    escrow.status = EscrowStatus::Disputed;
    write_escrow(e, escrow_id, &escrow);
    crate::events::DisputeRaised {
        claimant: claimant.clone(),
        escrow_id,
    }
    .publish(e);
}

/// Whether a dispute on `escrow_id` is still live.
///
/// A dispute is live until a ruling is final (an appeal was resolved or an
/// unappealed ruling lapsed) or the dispute expired. While it is live the
/// escrow must not be released or refunded.
pub fn is_dispute_open(e: &Env, escrow_id: u32) -> bool {
    match e
        .storage()
        .persistent()
        .get::<_, DisputeRecord>(&DataKey::Dispute(escrow_id))
    {
        Some(record) => {
            bump_persistent(e, &DataKey::Dispute(escrow_id));
            !record.is_final && record.status != DisputeStatus::Expired
        }
        None => false,
    }
}

/// The dispute record for `escrow_id`.
///
/// # Panics
///
/// With `DisputeNotFound` when no dispute is open over `escrow_id`.
pub fn get_dispute(e: &Env, escrow_id: u32) -> DisputeRecord {
    read_dispute(e, escrow_id)
}

/// Ruled on which of the escrow's parties wins.
///
/// Only the assigned resolver may rule, and only while there is no dispute in
/// flight. `winner` must be a party to the escrow. The first call records the
/// ruling and opens the appeal window without moving funds; calling again once
/// the window has lapsed declares the ruling final-world: the escrow is settled
/// and the winner is paid.
///
/// # Panics
///
/// With `Unauthorized: caller is not the assigned resolver`, `DisputeNotOpen`,
/// `EscrowNotFound` or `WinnerNotEscrowParty` on invalid input, and
/// `AppealWindowOpen` when called before the appeal window has lapsed.
pub fn resolve_dispute(e: &Env, resolver: Address, escrow_id: u32, winner: Address) {
    resolver.require_auth();
    let mut record = read_dispute(e, escrow_id);

    let assigned = record.resolver.clone().unwrap_or_else(|| get_arbiter(e));
    if assigned != resolver {
        panic!("Unauthorized: caller is not the assigned resolver");
    }
    if record.is_final {
        panic!(
            "DisputeFinal: escrow {} already has a final ruling",
            escrow_id
        );
    }

    let escrow = read_escrow(e, escrow_id);
    if winner != escrow.depositor && winner != escrow.beneficiary {
        panic!(
            "WinnerNotEscrowParty: {:?} is not a party to escrow {}",
            winner, escrow_id
        );
    }

    let ruling_ledger = e.ledger().sequence();
    match record.status {
        // First ruling: record the result and open the appeal window. The
        // escrow stays locked so an appeal can still block the payout.
        DisputeStatus::Open => {
            record.status = DisputeStatus::Resolved;
            record.resolved_ledger = ruling_ledger;
            record.winner = Some(winner.clone());
            write_dispute(e, escrow_id, &record);
            crate::events::DisputeResolved {
                resolver: resolver.clone(),
                escrow_id,
                winner: winner.clone(),
            }
            .publish(e);
        }
        // The ruling was already recorded and the appeal window has lapsed with
        // no appeal filed: finalize, settle and pay.
        DisputeStatus::Resolved => {
            if record.resolved_ledger == 0
                || ruling_ledger <= record.resolved_ledger + APPEAL_WINDOW_LEDGERS
            {
                panic!(
                    "AppealWindowOpen: escrow {} is still appealable until {}",
                    escrow_id,
                    record.resolved_ledger + APPEAL_WINDOW_LEDGERS
                );
            }
            let stored_winner = record.winner.as_ref().unwrap();
            if &winner != stored_winner {
                panic!(
                    "WinnerMismatch: ruling winner {:?} cannot be changed to {:?}",
                    stored_winner, winner
                );
            }
            record.is_final = true;
            write_dispute(e, escrow_id, &record);
            settle_escrow(e, escrow_id, stored_winner, &escrow);
        }
        _ => panic!(
            "DisputeNotOpen: escrow {} is in {:?}, not Open",
            escrow_id, record.status
        ),
    }
}

/// Escalated a ruling: the losing party asks for a final, unappealable ruling.
///
/// Only the losing party (a party to the escrow, not the declared winner) may
/// appeal, and only while a ruling is pending and inside the appeal window.
/// Payout stays blocked while the dispute is `Appealed`.
///
/// # Panics
///
/// With `DisputeFinal`, `DisputeNotAppealable`, `AppealWindowClosed` or
/// `AppealCallerNotParty`.
pub fn appeal_dispute(e: &Env, caller: &Address, escrow_id: u32) {
    caller.require_auth();
    let mut record = read_dispute(e, escrow_id);

    if record.is_final {
        panic!(
            "DisputeFinal: escrow {} already has a final ruling",
            escrow_id
        );
    }
    if record.status != DisputeStatus::Resolved {
        panic!(
            "DisputeNotAppealable: escrow {} is in {:?}, not Resolved",
            escrow_id, record.status
        );
    }
    let ruling_ledger = record.resolved_ledger;
    if ruling_ledger == 0 || e.ledger().sequence() > ruling_ledger + APPEAL_WINDOW_LEDGERS {
        panic!(
            "AppealWindowClosed: appeal for escrow {} closed at ledger {}",
            escrow_id,
            ruling_ledger + APPEAL_WINDOW_LEDGERS
        );
    }

    let escrow = read_escrow(e, escrow_id);
    if caller != &escrow.depositor && caller != &escrow.beneficiary {
        panic!(
            "AppealCallerNotParty: {:?} is not a party to escrow {}",
            caller, escrow_id
        );
    }
    if record.winner.as_ref() == Some(caller) {
        panic!("AppealCallerNotParty: the declared winner cannot appeal");
    }

    record.status = DisputeStatus::Appealed;
    write_dispute(e, escrow_id, &record);
    crate::events::DisputeAppealed {
        caller: caller.clone(),
        escrow_id,
    }
    .publish(e);
}

/// Handed down the final, unappealable ruling on an appealed dispute.
///
/// Settles the escrow in the winner's favour and marks the dispute final; a
/// second appeal on the same escrow is rejected.
///
/// # Panics
///
/// With `Unauthorized: caller is not the assigned resolver`, `DisputeNotAppealed`,
/// `EscrowNotFound` or `WinnerNotEscrowParty`.
pub fn resolve_appeal(e: &Env, resolver: Address, escrow_id: u32, winner: Address) {
    resolver.require_auth();
    let mut record = read_dispute(e, escrow_id);

    let assigned = record.resolver.clone().unwrap_or_else(|| get_arbiter(e));
    if assigned != resolver {
        panic!("Unauthorized: caller is not the assigned resolver");
    }
    if record.status != DisputeStatus::Appealed {
        panic!(
            "DisputeNotAppealed: escrow {} is in {:?}, not Appealed",
            escrow_id, record.status
        );
    }

    let escrow = read_escrow(e, escrow_id);
    if winner != escrow.depositor && winner != escrow.beneficiary {
        panic!(
            "WinnerNotEscrowParty: {:?} is not a party to escrow {}",
            winner, escrow_id
        );
    }

    record.status = DisputeStatus::Resolved;
    record.resolved_ledger = e.ledger().sequence();
    record.winner = Some(winner.clone());
    record.is_final = true;
    write_dispute(e, escrow_id, &record);

    settle_escrow(e, escrow_id, &winner, &escrow);
    crate::events::DisputeFinalised {
        resolver: resolver.clone(),
        escrow_id,
        winner: winner.clone(),
    }
    .publish(e);
}

/// Expires a dispute once the escrow's deadline has passed, refunding the depositor.
///
/// Can be called by the depositor, beneficiary, or admin.
pub fn expire_dispute(e: &Env, caller: &Address, escrow_id: u32) {
    caller.require_auth();
    let mut record = read_dispute(e, escrow_id);
    if record.is_final || record.status == DisputeStatus::Expired {
        panic!(
            "DisputeFinal: dispute on escrow {} is already final",
            escrow_id
        );
    }
    let escrow = read_escrow(e, escrow_id);
    if caller != &escrow.depositor
        && caller != &escrow.beneficiary
        && !crate::admin::is_admin(e, caller)
    {
        panic!(
            "Unauthorized: only the depositor, beneficiary, or admin may expire dispute on escrow {}",
            escrow_id
        );
    }
    if e.ledger().sequence() <= escrow.deadline_ledger {
        panic!(
            "DisputeNotExpired: escrow {} deadline ledger {} has not passed",
            escrow_id, escrow.deadline_ledger
        );
    }

    record.status = DisputeStatus::Expired;
    record.is_final = true;
    write_dispute(e, escrow_id, &record);

    settle_escrow(e, escrow_id, &escrow.depositor, &escrow);
}

/// Settles the escrow in `winner`'s favour and pays them the escrowed amount.
///
/// Moves the escrowed amount to the winner when the winner is the beneficiary
/// (a refund simply leaves the funds with the depositor), and marks the escrow
/// `Released` or `Refunded`.
fn settle_escrow(e: &Env, escrow_id: u64, winner: &Address, escrow: &EscrowRecord) {
    let amount = escrow.amount;
    crate::validation::require_positive_amount(amount);

    crate::escrow::pay(e, &escrow.token, winner, amount);
    crate::escrow::decrease_value_locked(e, amount);

    let status = if winner == &escrow.beneficiary {
        EscrowStatus::Released
    } else {
        EscrowStatus::Refunded
    };
    write_escrow(
        e,
        escrow_id,
        &EscrowRecord {
            status,
            amount: 0,
            ..escrow.clone()
        },
    );
}
