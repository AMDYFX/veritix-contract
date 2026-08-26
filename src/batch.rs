use soroban_sdk::{symbol_short, Address, Env, Vec};

pub fn approve_batch(e: &Env, from: Address, approvals: Vec<(Address, i128, u32)>) {
    from.require_auth();
    for i in 0..approvals.len() {
        let (spender, amount, expiry) = approvals.get(i).unwrap();
        crate::allowance::write_allowance(e, &from, &spender, amount, expiry);
        e.events().publish(
            (symbol_short!("approve"), from.clone(), spender.clone()),
            (amount, expiry),
        );
    }
}

pub fn clawback_batch(e: &Env, admin: Address, clawbacks: Vec<(Address, i128)>) {
    crate::admin::check_admin(e, &admin);
    if let Some(cosigner) = crate::admin::read_clawback_cosigner(e) {
        cosigner.require_auth();
    }
    for i in 0..clawbacks.len() {
        let (from, amount) = clawbacks.get(i).unwrap();
        crate::balance::spend_balance(e, &from, amount);
    }
}

pub fn mint_batch(e: &Env, admin: Address, mints: Vec<(Address, i128)>) -> i128 {
    crate::admin::check_admin(e, &admin);
    let mut total: i128 = 0;
    for i in 0..mints.len() {
        let (to, amount) = mints.get(i).unwrap();
        crate::balance::receive_balance(e, &to, amount);
        total += amount;
        e.events().publish(
            (symbol_short!("mint"), admin.clone(), to.clone()),
            amount,
        );
    }
    e.events().publish(
        (symbol_short!("btch_mnt"), admin.clone()),
        total,
    );
    total
}
