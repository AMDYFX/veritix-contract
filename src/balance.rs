use crate::storage_types::DataKey;
use soroban_sdk::Env;

pub fn read_supply(e: &Env) -> i128 {
    e.storage()
        .persistent()
        .get(&DataKey::TotalSupply)
        .unwrap_or(0)
}

pub fn read_max_supply(e: &Env) -> i128 {
    e.storage()
        .persistent()
        .get(&DataKey::MaxSupply)
        .unwrap_or(0)
}

pub fn increase_supply(e: &Env, amount: i128) {
    let supply = read_supply(e);
    let new_supply = supply.checked_add(amount).expect("supply overflow");
    let max = read_max_supply(e);
    if max > 0 && new_supply > max {
        panic!("SupplyCap: minting would exceed max supply of {}", max);
    }
    e.storage()
        .persistent()
        .set(&DataKey::TotalSupply, &new_supply);
}

pub fn decrease_supply(e: &Env, amount: i128) {
    let supply = read_supply(e);
    let new_supply = supply.checked_sub(amount).expect("supply underflow");
    e.storage()
        .persistent()
        .set(&DataKey::TotalSupply, &new_supply);
}

use soroban_sdk::Address;

pub fn balance_of(e: &Env, account: &Address) -> i128 {
    e.storage()
        .persistent()
        .get(&DataKey::BalanceOf(account.clone()))
        .unwrap_or(0)
}

pub fn spendable_balance(e: &Env, account: &Address) -> i128 {
    if is_frozen(e, account) {
        return 0;
    }
    balance_of(e, account)
}

pub fn is_frozen(e: &Env, account: &Address) -> bool {
    e.storage()
        .persistent()
        .get(&DataKey::Frozen(account.clone()))
        .unwrap_or(false)
}

pub fn set_authorized(e: &Env, admin: &Address, account: &Address, authorized: bool) {
    crate::admin::check_admin(e, admin);
    if authorized {
        e.storage()
            .persistent()
            .remove(&DataKey::Frozen(account.clone()));
    } else {
        e.storage()
            .persistent()
            .set(&DataKey::Frozen(account.clone()), &true);
    }
}

pub fn burn_from(e: &Env, spender: &Address, from: &Address, amount: i128) {
    spender.require_auth();
    assert!(amount > 0, "amount must be positive");
    crate::allowance::spend_allowance(e, from, spender, amount);
    let balance = balance_of(e, from);
    assert!(balance >= amount, "insufficient balance");
    let new_balance = balance - amount;
    if new_balance == 0 {
        e.storage()
            .persistent()
            .remove(&DataKey::BalanceOf(from.clone()));
    } else {
        e.storage()
            .persistent()
            .set(&DataKey::BalanceOf(from.clone()), &new_balance);
    }
    decrease_supply(e, amount);
}

pub fn read_balance(e: &Env, account: &Address) -> i128 {
    balance_of(e, account)
}

pub fn add_balance(e: &Env, account: &Address, amount: i128) {
    receive_balance(e, account, amount);
}

pub fn receive_balance(e: &Env, account: &Address, amount: i128) {
    if *account == e.current_contract_address() {
        return;
    }
    let current = balance_of(e, account);
    let new_balance = current.checked_add(amount).expect("balance overflow");
    e.storage()
        .persistent()
        .set(&DataKey::BalanceOf(account.clone()), &new_balance);
    update_holder_set(e, account);
}

pub fn spend_balance(e: &Env, account: &Address, amount: i128) {
    let current = balance_of(e, account);
    assert!(current >= amount, "insufficient balance");
    let new_balance = current - amount;
    let key = DataKey::BalanceOf(account.clone());
    if new_balance == 0 {
        e.storage().persistent().remove(&key);
    } else {
        e.storage().persistent().set(&key, &new_balance);
    }
    update_holder_set(e, account);
}

pub fn update_holder_set(e: &Env, addr: &Address) {
    if *addr == e.current_contract_address() {
        return;
    }
    let bal = balance_of(e, addr);
    let mut count: u32 = e
        .storage()
        .persistent()
        .get(&DataKey::HolderCount)
        .unwrap_or(0);
    let mut holders: soroban_sdk::Vec<Address> = e
        .storage()
        .persistent()
        .get(&DataKey::HolderSet)
        .unwrap_or(soroban_sdk::Vec::new(e));
    let mut exists = false;
    let mut idx = 0;
    for i in 0..holders.len() {
        if holders.get(i).unwrap() == *addr {
            exists = true;
            idx = i;
            break;
        }
    }
    if bal > 0 && !exists {
        holders.push_back(addr.clone());
        count += 1;
        e.storage().persistent().set(&DataKey::HolderSet, &holders);
        e.storage().persistent().set(&DataKey::HolderCount, &count);
    } else if bal == 0 && exists {
        holders.remove(idx);
        if count > 0 {
            count -= 1;
        }
        e.storage().persistent().set(&DataKey::HolderSet, &holders);
        e.storage().persistent().set(&DataKey::HolderCount, &count);
    }
}
