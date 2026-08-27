use crate::escrow::load_record;
use crate::storage_types::DataKey;
use crate::storage_types::ResolverStats;
use soroban_sdk::{token, Address, Env, Vec};

pub fn set_arbiter(e: &Env, arbiter: &Address) {
    arbiter.require_auth();
    e.storage().persistent().set(&DataKey::Arbiter, arbiter);
}

pub fn get_arbiter(e: &Env) -> Address {
    e.storage()
        .persistent()
        .get(&DataKey::Arbiter)
        .expect("arbiter not set")
}

pub fn open_dispute(e: Env, claimant: Address, escrow_id: u32, dispute_id: u32) {
    claimant.require_auth();
    let record = load_record(&e, escrow_id);
    assert!(
        claimant == record.depositor || claimant == record.beneficiary,
        "only depositor or beneficiary can open dispute"
    );
    let mut disputes = get_disputes_by_claimant(e.clone(), claimant.clone());
    disputes.push_back(dispute_id);
    e.storage()
        .persistent()
        .set(&DataKey::ClaimantDisputes(claimant), &disputes);
}

pub fn raise_dispute(e: &Env, caller: &Address, escrow_id: u32) {
    caller.require_auth();

    let record = load_record(e, escrow_id);
    assert!(
        !record.released && !record.refunded,
        "escrow already settled"
    );
    assert!(
        *caller == record.depositor || *caller == record.beneficiary,
        "only depositor or beneficiary can raise dispute"
    );

    e.storage()
        .persistent()
        .set(&DataKey::EscrowDispute(escrow_id), &true);
    e.storage()
        .persistent()
        .set(&DataKey::DisputeOpenedAt(escrow_id), &e.ledger().sequence());

    let mut disputes = get_disputes_by_claimant(e.clone(), caller.clone());
    disputes.push_back(escrow_id);
    e.storage()
        .persistent()
        .set(&DataKey::ClaimantDisputes(caller.clone()), &disputes);

    e.events().publish(
        (soroban_sdk::symbol_short!("dispute"),),
        (caller.clone(), escrow_id),
    );
}

pub fn resolve_dispute(e: &Env, resolver: &Address, escrow_id: u32, winner: &Address) {
    // #522: Mediation fee
    let mediation_fee_bps: u32 = e
        .storage()
        .persistent()
        .get(&DataKey::MediationFeeBps)
        .unwrap_or(0);
    let mut transfer_amount: i128;
    let arbiter = get_arbiter(e);
    if *resolver != arbiter {
        panic!("Unauthorized: only the arbiter can resolve disputes");
    }
    resolver.require_auth();

    let is_disputed: bool = e
        .storage()
        .persistent()
        .get(&DataKey::EscrowDispute(escrow_id))
        .unwrap_or(false);
    assert!(is_disputed, "escrow is not under dispute");

    let mut record = load_record(e, escrow_id);
    assert!(
        !record.released && !record.refunded,
        "escrow already settled"
    );

    let is_for_beneficiary = *winner == record.beneficiary;
    let is_for_depositor = *winner == record.depositor;
    assert!(
        is_for_beneficiary || is_for_depositor,
        "winner must be depositor or beneficiary"
    );

    let token_client = token::Client::new(e, &record.token);

    if is_for_beneficiary {
        record.released = true;
        record.released_amount = record.amount;
        crate::escrow::save_record(e, &record);

        let remaining = record.amount;
        if remaining > 0 {
            transfer_amount = remaining;
            if mediation_fee_bps > 0 && *resolver != e.current_contract_address() {
                let mediation_fee = remaining * mediation_fee_bps as i128 / 10000;
                if mediation_fee > 0 {
                    token_client.transfer(&e.current_contract_address(), resolver, &mediation_fee);
                    transfer_amount = remaining - mediation_fee;
                }
            }
            if transfer_amount > 0 {
                token_client.transfer(
                    &e.current_contract_address(),
                    &record.beneficiary,
                    &transfer_amount,
                );
            }
        }
    } else {
        record.refunded = true;
        crate::escrow::save_record(e, &record);

        let refundable = record.amount - record.released_amount;
        if refundable > 0 {
            transfer_amount = refundable;
            if mediation_fee_bps > 0 && *resolver != e.current_contract_address() {
                let mediation_fee = refundable * mediation_fee_bps as i128 / 10000;
                if mediation_fee > 0 {
                    token_client.transfer(&e.current_contract_address(), resolver, &mediation_fee);
                    transfer_amount = refundable - mediation_fee;
                }
            }
            if transfer_amount > 0 {
                token_client.transfer(
                    &e.current_contract_address(),
                    &record.depositor,
                    &transfer_amount,
                );
            }
        }
    }

    e.storage()
        .persistent()
        .remove(&DataKey::EscrowDispute(escrow_id));

    update_resolver_stats(e, resolver, is_for_beneficiary);

    e.events().publish(
        (soroban_sdk::symbol_short!("dis_res"),),
        (resolver.clone(), escrow_id, winner.clone()),
    );
}

fn update_resolver_stats(e: &Env, resolver: &Address, for_beneficiary: bool) {
    let mut stats: ResolverStats = e
        .storage()
        .persistent()
        .get(&DataKey::ResolverStats(resolver.clone()))
        .unwrap_or(ResolverStats {
            resolver: resolver.clone(),
            total_resolved: 0,
            for_beneficiary: 0,
            for_depositor: 0,
        });

    stats.total_resolved += 1;
    if for_beneficiary {
        stats.for_beneficiary += 1;
    } else {
        stats.for_depositor += 1;
    }

    e.storage()
        .persistent()
        .set(&DataKey::ResolverStats(resolver.clone()), &stats);
}

pub fn get_resolver_stats(e: &Env, resolver: &Address) -> ResolverStats {
    e.storage()
        .persistent()
        .get(&DataKey::ResolverStats(resolver.clone()))
        .unwrap_or(ResolverStats {
            resolver: resolver.clone(),
            total_resolved: 0,
            for_beneficiary: 0,
            for_depositor: 0,
        })
}
pub fn get_disputes_by_claimant(e: Env, claimant: Address) -> Vec<u32> {
    e.storage()
        .persistent()
        .get(&DataKey::ClaimantDisputes(claimant))
        .unwrap_or(Vec::new(&e))
}

// ── #670/#671: Dispute appeal and expiry ─────────────────────────────────────

pub const DISPUTE_APPEAL_WINDOW: u32 = 17280;
pub const DISPUTE_EXPIRE_AFTER: u32 = 34560;

/// A party to a dispute can appeal (escalate) the dispute within a short window
/// while it is still open. This marks a pending appeal so the dispute is handled
/// through the appeal path rather than a normal resolution.
pub fn appeal_dispute(e: &Env, caller: &Address, escrow_id: u32) {
    caller.require_auth();

    let record = load_record(e, escrow_id);
    assert!(
        !record.released && !record.refunded,
        "escrow already settled"
    );
    assert!(
        *caller == record.depositor || *caller == record.beneficiary,
        "only depositor or beneficiary can appeal"
    );

    let is_disputed: bool = e
        .storage()
        .persistent()
        .get(&DataKey::EscrowDispute(escrow_id))
        .unwrap_or(false);
    assert!(is_disputed, "escrow is not under dispute");

    let opened_at: u32 = e
        .storage()
        .persistent()
        .get(&DataKey::DisputeOpenedAt(escrow_id))
        .unwrap_or(0);
    let has_appeal: bool = e
        .storage()
        .persistent()
        .get(&DataKey::DisputeAppeal(escrow_id))
        .unwrap_or(false);
    assert!(!has_appeal, "dispute already has a pending appeal");

    // Appeals only allowed shortly after the dispute is opened.
    assert!(
        e.ledger().sequence() <= opened_at + DISPUTE_APPEAL_WINDOW,
        "appeal window has expired"
    );

    e.storage()
        .persistent()
        .set(&DataKey::DisputeAppeal(escrow_id), &true);

    e.events().publish(
        (soroban_sdk::symbol_short!("appeal"),),
        (caller.clone(), escrow_id),
    );
}

/// The arbiter settles the escrow after an appeal has been raised, honoring the
/// appeal by sending the funds to the declared winner.
pub fn resolve_appeal(e: &Env, resolver: &Address, escrow_id: u32, winner: &Address) {
    let arbiter = get_arbiter(e);
    if *resolver != arbiter {
        panic!("Unauthorized: only the arbiter can resolve disputes");
    }
    resolver.require_auth();

    let has_appeal: bool = e
        .storage()
        .persistent()
        .get(&DataKey::DisputeAppeal(escrow_id))
        .unwrap_or(false);
    assert!(has_appeal, "no pending appeal to resolve");

    let is_disputed: bool = e
        .storage()
        .persistent()
        .get(&DataKey::EscrowDispute(escrow_id))
        .unwrap_or(false);
    assert!(is_disputed, "escrow is not under dispute");

    let mut record = load_record(e, escrow_id);
    assert!(
        !record.released && !record.refunded,
        "escrow already settled"
    );
    let is_for_beneficiary = *winner == record.beneficiary;
    let is_for_depositor = *winner == record.depositor;
    assert!(
        is_for_beneficiary || is_for_depositor,
        "winner must be depositor or beneficiary"
    );

    let token_client = token::Client::new(e, &record.token);

    if is_for_beneficiary {
        record.released = true;
        record.released_amount = record.amount;
        crate::escrow::save_record(e, &record);
        let remaining = record.amount;
        if remaining > 0 {
            token_client.transfer(
                &e.current_contract_address(),
                &record.beneficiary,
                &remaining,
            );
        }
    } else {
        record.refunded = true;
        crate::escrow::save_record(e, &record);
        let refundable = record.amount - record.released_amount;
        if refundable > 0 {
            token_client.transfer(
                &e.current_contract_address(),
                &record.depositor,
                &refundable,
            );
        }
    }

    e.storage()
        .persistent()
        .remove(&DataKey::EscrowDispute(escrow_id));
    e.storage()
        .persistent()
        .remove(&DataKey::DisputeAppeal(escrow_id));
    e.storage()
        .persistent()
        .remove(&DataKey::DisputeOpenedAt(escrow_id));

    update_resolver_stats(e, resolver, is_for_beneficiary);

    e.events().publish(
        (soroban_sdk::symbol_short!("app_res"),),
        (resolver.clone(), escrow_id, winner.clone()),
    );
}

/// Any party may expire a dispute that has remained open too long without a
/// resolution, releasing the escrow so it can be refunded or released normally.
pub fn expire_dispute(e: &Env, caller: &Address, escrow_id: u32) {
    caller.require_auth();

    let record = load_record(e, escrow_id);
    assert!(
        !record.released && !record.refunded,
        "escrow already settled"
    );
    assert!(
        *caller == record.depositor || *caller == record.beneficiary,
        "only depositor or beneficiary can expire a dispute"
    );

    let is_disputed: bool = e
        .storage()
        .persistent()
        .get(&DataKey::EscrowDispute(escrow_id))
        .unwrap_or(false);
    assert!(is_disputed, "escrow is not under dispute");

    let opened_at: u32 = e
        .storage()
        .persistent()
        .get(&DataKey::DisputeOpenedAt(escrow_id))
        .unwrap_or(0);
    assert!(
        e.ledger().sequence() >= opened_at + DISPUTE_EXPIRE_AFTER,
        "dispute has not been open long enough to expire"
    );

    e.storage()
        .persistent()
        .remove(&DataKey::EscrowDispute(escrow_id));
    e.storage()
        .persistent()
        .remove(&DataKey::DisputeAppeal(escrow_id));
    e.storage()
        .persistent()
        .remove(&DataKey::DisputeOpenedAt(escrow_id));

    e.events().publish(
        (
            soroban_sdk::symbol_short!("dis_exp"),
            caller.clone(),
            escrow_id,
        ),
        (e.ledger().sequence(),),
    );
}
