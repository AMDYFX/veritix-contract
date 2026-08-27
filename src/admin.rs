use crate::storage_types::DataKey;
use soroban_sdk::{Address, Env};

pub const ADMIN_ACTIVATION_DELAY: u32 = 17280;

pub fn validate_admin_address(_e: &Env, _admin: &Address) {
    // Basic validation — ensure address is usable.
    // In Soroban all addresses are structurally valid;
    // this guard exists so callers can extend checks later.
}

pub fn check_admin(e: &Env, caller: &Address) {
    let admin: Address = e
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .expect("admin not set");

    if admin != *caller {
        panic!("Unauthorized: caller is not the contract admin");
    }

    let admin_active_after: u32 = e
        .storage()
        .persistent()
        .get(&DataKey::AdminActiveAfter)
        .unwrap_or(0);

    if e.ledger().sequence() < admin_active_after {
        panic!(
            "AdminNotActive yet — new admin becomes active after ledger {}",
            admin_active_after
        );
    }

    caller.require_auth();
}

pub fn is_admin(e: &Env, caller: &Address) -> bool {
    let admin: Address = e
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .expect("admin not set");

    admin == *caller
}

pub fn is_initialized(e: &Env) -> bool {
    e.storage().persistent().has(&DataKey::Admin)
}

pub fn require_initialized(e: &Env) {
    if !is_initialized(e) {
        panic!("NotInitialized: call initialize first");
    }
}

// ── #451: One-step admin change with time-delay ──────────────────────────────

pub fn transfer_ownership(e: &Env, new_admin: &Address) {
    let current_admin: Address = e
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .expect("admin not set");
    current_admin.require_auth();

    assert!(*new_admin != current_admin, "cannot propose current admin");

    e.storage()
        .persistent()
        .set(&DataKey::ProposedAdmin, new_admin);

    e.events().publish(
        (soroban_sdk::symbol_short!("ownership"),),
        (current_admin, new_admin.clone()),
    );
}

pub fn accept_admin(e: &Env, new_admin: &Address) {
    let proposed: Address = e
        .storage()
        .persistent()
        .get(&DataKey::ProposedAdmin)
        .expect("no admin proposal pending");

    if proposed != *new_admin {
        panic!("NotProposed: caller is not the proposed admin");
    }

    new_admin.require_auth();

    e.storage().persistent().set(&DataKey::Admin, new_admin);

    let activation_ledger = e.ledger().sequence() + ADMIN_ACTIVATION_DELAY;
    e.storage()
        .persistent()
        .set(&DataKey::AdminActiveAfter, &activation_ledger);

    e.storage().persistent().remove(&DataKey::ProposedAdmin);

    e.events().publish(
        (soroban_sdk::symbol_short!("admin_set"),),
        (new_admin.clone(), activation_ledger),
    );
}

pub fn admin_active_after_ledger(e: &Env) -> u32 {
    e.storage()
        .persistent()
        .get(&DataKey::AdminActiveAfter)
        .unwrap_or(0)
}

pub fn read_clawback_cosigner(e: &Env) -> Option<Address> {
    e.storage().persistent().get(&DataKey::ClawbackCosigner)
}

pub fn set_clawback_cosigner(e: &Env, admin: &Address, cosigner: &Address) {
    check_admin(e, admin);
    e.storage()
        .persistent()
        .set(&DataKey::ClawbackCosigner, cosigner);
}
