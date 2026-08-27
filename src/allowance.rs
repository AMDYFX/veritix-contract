use crate::storage_types::DataKey;
use soroban_sdk::{contracttype, Address, Env};

#[contracttype]
#[derive(Clone)]
pub struct Allowance {
    pub amount: i128,
    pub expiration_ledger: u32,
}

pub fn write_allowance(
    e: &Env,
    from: &Address,
    spender: &Address,
    amount: i128,
    expiration_ledger: u32,
) {
    let key = DataKey::Allowance(from.clone(), spender.clone());
    if amount == 0 {
        e.storage().persistent().remove(&key);
        write_owner_allowance_index(e, from, spender, false);
    } else {
        let allowance = Allowance {
            amount,
            expiration_ledger,
        };
        e.storage().persistent().set(&key, &allowance);
        write_owner_allowance_index(e, from, spender, true);
    }
}

pub fn create_allowance(
    e: &Env,
    from: &Address,
    spender: &Address,
    amount: i128,
    expiration_ledger: u32,
) {
    track_spender(e, from, spender);
    write_allowance(e, from, spender, amount, expiration_ledger);
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

    e.storage()
        .persistent()
        .set(&DataKey::ProposedAdmin, new_admin);

    e.events().publish(
        (soroban_sdk::symbol_short!("ownership"),),
        (current_admin, new_admin.clone()),
    );
}

pub fn read_allowance(e: &Env, from: &Address, spender: &Address) -> Allowance {
    let key = DataKey::Allowance(from.clone(), spender.clone());
    if let Some(allowance) = e.storage().persistent().get::<_, Allowance>(&key) {
        if allowance.expiration_ledger < e.ledger().sequence() && allowance.expiration_ledger != 0 {
            e.storage().persistent().remove(&key);
            write_owner_allowance_index(e, from, spender, false);
            return Allowance {
                amount: 0,
                expiration_ledger: 0,
            };
        }
        allowance
    } else {
        Allowance {
            amount: 0,
            expiration_ledger: 0,
        }
    }
}

pub fn spend_allowance(e: &Env, from: &Address, spender: &Address, amount: i128) {
    let allowance = read_allowance(e, from, spender);
    if allowance.expiration_ledger < e.ledger().sequence() {
        panic!("allowance expired");
    }
    if allowance.amount < amount {
        panic!("insufficient allowance");
    }
    let new_amount = allowance.amount - amount;
    write_allowance(e, from, spender, new_amount, allowance.expiration_ledger);
}

pub fn increase_allowance(e: &Env, from: &Address, spender: &Address, amount: i128) {
    from.require_auth();
    assert!(amount > 0, "amount must be positive");
    let current = read_allowance(e, from, spender);
    let new_amount = current.amount + amount;
    track_spender(e, from, spender);
    write_allowance(e, from, spender, new_amount, current.expiration_ledger);
}

pub fn decrease_allowance(e: &Env, from: &Address, spender: &Address, amount: i128) {
    from.require_auth();
    assert!(amount > 0, "amount must be positive");
    let current = read_allowance(e, from, spender);
    if current.amount <= amount {
        write_allowance(e, from, spender, 0, current.expiration_ledger);
    } else {
        let new_amount = current.amount - amount;
        write_allowance(e, from, spender, new_amount, current.expiration_ledger);
    }
}

pub fn revoke_all_allowances(e: &Env, from: &Address) {
    let spenders: soroban_sdk::Vec<Address> = e
        .storage()
        .persistent()
        .get(&DataKey::AllowanceSpenders(from.clone()))
        .unwrap_or(soroban_sdk::Vec::new(e));
    for i in 0..spenders.len() {
        if let Some(spender) = spenders.get(i) {
            e.storage()
                .persistent()
                .remove(&DataKey::Allowance(from.clone(), spender));
        }
    }
    e.storage()
        .persistent()
        .remove(&DataKey::AllowanceSpenders(from.clone()));
}

pub fn track_spender(e: &Env, from: &Address, spender: &Address) {
    let mut spenders: soroban_sdk::Vec<Address> = e
        .storage()
        .persistent()
        .get(&DataKey::AllowanceSpenders(from.clone()))
        .unwrap_or(soroban_sdk::Vec::new(e));
    let mut found = false;
    for i in 0..spenders.len() {
        if let Some(s) = spenders.get(i) {
            if s == *spender {
                found = true;
                break;
            }
        }
    }
    if !found {
        spenders.push_back(spender.clone());
        e.storage()
            .persistent()
            .set(&DataKey::AllowanceSpenders(from.clone()), &spenders);
    }
}

pub fn write_owner_allowance_index(e: &Env, from: &Address, spender: &Address, add: bool) {
    let spenders: soroban_sdk::Vec<Address> = e
        .storage()
        .persistent()
        .get(&DataKey::AllowanceSpenders(from.clone()))
        .unwrap_or(soroban_sdk::Vec::new(e));
    if add {
        track_spender(e, from, spender);
    } else {
        let mut updated = soroban_sdk::Vec::new(e);
        for i in 0..spenders.len() {
            if let Some(s) = spenders.get(i) {
                if s != *spender {
                    updated.push_back(s);
                }
            }
        }
        if updated.is_empty() {
            e.storage()
                .persistent()
                .remove(&DataKey::AllowanceSpenders(from.clone()));
        } else {
            e.storage()
                .persistent()
                .set(&DataKey::AllowanceSpenders(from.clone()), &updated);
        }
    }
}

pub fn get_allowances_for_spender(e: &Env, from: &Address) -> soroban_sdk::Vec<Address> {
    e.storage()
        .persistent()
        .get(&DataKey::AllowanceSpenders(from.clone()))
        .unwrap_or(soroban_sdk::Vec::new(e))
}
