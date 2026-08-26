use soroban_sdk::{token, Env, Address, Vec};
use crate::storage_types::DataKey;
use crate::escrow::load_record;
use crate::storage_types::ResolverStats;

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

    e.events().publish(
        (soroban_sdk::symbol_short!("dispute"),),
        (caller.clone(), escrow_id),
    );
}

pub fn resolve_dispute(e: &Env, resolver: &Address, escrow_id: u32, winner: &Address) {
    // #522: Mediation fee
    let mediation_fee_bps: u32 = e.storage().persistent().get(&DataKey::MediationFeeBps).unwrap_or(0);
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
                token_client.transfer(&e.current_contract_address(), &record.beneficiary, &transfer_amount);
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
                token_client.transfer(&e.current_contract_address(), &record.depositor, &transfer_amount);
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
