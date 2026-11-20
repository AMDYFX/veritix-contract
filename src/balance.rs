use crate::control;
use crate::events::{self, Burn, Clawback, Mint, Transfer};
use crate::storage_types::DataKey;
use crate::validation::require_positive_amount;
use soroban_sdk::{Address, Bytes, Env, Vec};

/// Tokens in circulation. An absent key means the contract has minted nothing.
///
/// The supply counters are contract-wide configuration, so they live in
/// instance storage with the admin (see `docs/ttl.md`).
pub fn total_supply(e: &Env) -> i128 {
    e.storage()
        .instance()
        .get(&DataKey::TotalSupply)
        .unwrap_or(0)
}

/// The configured hard cap, or 0 when supply is uncapped.
pub fn max_supply(e: &Env) -> i128 {
    e.storage().instance().get(&DataKey::MaxSupply).unwrap_or(0)
}

/// Tokens held by `account`. An absent key means a zero balance.
pub fn balance_of(e: &Env, account: &Address) -> i128 {
    e.storage()
        .persistent()
        .get(&DataKey::BalanceOf(account.clone()))
        .unwrap_or(0)
}

/// The portion of `account`'s balance held by an active escrow.
pub fn escrow_locked(e: &Env, account: &Address) -> i128 {
    e.storage()
        .persistent()
        .get(&DataKey::EscrowLocked(account.clone()))
        .unwrap_or(0)
}

/// Tokens `account` may actually move: its balance less anything locked in an
/// active escrow, and `0` outright while the account is frozen.
///
/// Freezing blocks movement without confiscating, so the balance stays visible
/// through [`balance_of`] while the spendable figure reports what the owner can
/// still do with it.
pub fn spendable_balance(e: &Env, account: &Address) -> i128 {
    if control::is_frozen(e, account) {
        return 0;
    }
    balance_of(e, account).saturating_sub(escrow_locked(e, account))
}

// ---- Holder set -----------------------------------------------------------

/// Every account that currently holds a non-zero balance, in first-credit order.
///
/// The set is maintained at the only two places a balance crosses zero —
/// [`credit`] and [`debit`] — so every path that moves tokens (transfer, mint,
/// burn, clawback, dispute settlement) keeps it in step without having to
/// remember to.
pub(crate) fn holders(e: &Env) -> Vec<Address> {
    e.storage()
        .persistent()
        .get(&DataKey::Holders)
        .unwrap_or_else(|| Vec::new(e))
}

/// Records `account` as a holder unless it already is.
///
/// A repeat credit must not append a second entry: the set is what
/// `total_holders` counts and `get_holders` lists, and a duplicate would make
/// both lie and make removal at zero ambiguous.
fn add_holder(e: &Env, account: &Address) {
    let mut index = holders(e);
    if !index.contains(account.clone()) {
        index.push_back(account.clone());
        e.storage().persistent().set(&DataKey::Holders, &index);
    }
}


/// Number of accounts currently holding a non-zero balance.
pub fn total_holders(e: &Env) -> u32 {
    holders(e).len()
}

/// The largest page returned by [`holders_page`].
pub const MAX_HOLDER_PAGE: u32 = 50;

/// Every current holder, in first-credit order.
///
/// The count of holders grows with the number of users, so
/// [`holders_page`] is the bounded read for a client that must not pull
/// the whole set in one call.
pub fn get_holders(e: &Env) -> Vec<Address> {
    holders(e)
}

/// A page of holders, starting at `offset` and holding at most `limit`
/// (capped at [`MAX_HOLDER_PAGE`]) entries.
pub fn holders_page(e: &Env, offset: u32, limit: u32) -> Vec<Address> {
    let all = holders(e);
    let limit = if limit > MAX_HOLDER_PAGE {
        MAX_HOLDER_PAGE
    } else {
        limit
    };
    let end = core::cmp::min(offset.saturating_add(limit), all.len());
    let mut out = Vec::new(e);
    let mut i = offset;
    while i < end {
        if let Some(holder) = all.get(i) {
            out.push_back(holder);
        }
        i += 1;
    }
    out
}

/// Credits `amount` to `account` without touching total supply.
///
/// Callers that bring new tokens into circulation must also call
/// [`increase_supply`] so the two ledgers cannot drift apart.
///
/// An account's first credit also enters it into the holder set; a later
/// credit to a balance that had been drained to zero re-enters it.
pub fn credit(e: &Env, account: &Address, amount: i128) {
    let previous = balance_of(e, account);
    let new_balance = previous
        .checked_add(amount)
        .unwrap_or_else(|| panic!("BalanceOverflow: {} credit would overflow i128", amount));
    e.storage()
        .persistent()
        .set(&DataKey::BalanceOf(account.clone()), &new_balance);
    if previous == 0 {
        add_holder(e, account);
    }
}

/// Debits `amount` from `account`.
///
/// An account drained to zero leaves the holder set on the same call: a zero
/// balance and membership are two views of one fact, and letting them drift
/// would make `total_holders` count accounts that hold nothing.
///
/// # Panics
///
/// Panics with `InsufficientBalance` when the account holds less than `amount`.
pub fn debit(e: &Env, account: &Address, amount: i128) {
    let balance = balance_of(e, account);
    if balance < amount {
        panic!(
            "InsufficientBalance: {} available, {} required",
            balance, amount
        );
    }
    let new_balance = balance - amount;
    if new_balance == 0 {
        e.storage()
            .persistent()
            .remove(&DataKey::BalanceOf(account.clone()));
        remove_holder(e, account);
    } else {
        e.storage()
            .persistent()
            .set(&DataKey::BalanceOf(account.clone()), &new_balance);
    }
}

/// Adds `amount` to total supply, enforcing the configured cap.
///
/// A cap of 0 is the "no cap" sentinel: `max_supply` is unset until the
/// initializer opts into a limit, and an absent cap must not block minting.
///
/// # Panics
///
/// Panics with `SupplyCapExceeded` when the cap is set and the new total would
/// exceed it, and with `SupplyOverflow` on an `i128` overflow.
pub fn increase_supply(e: &Env, amount: i128) {
    let new_supply = total_supply(e)
        .checked_add(amount)
        .unwrap_or_else(|| panic!("SupplyOverflow: minting {} would overflow i128", amount));
    let cap = max_supply(e);
    if cap > 0 && new_supply > cap {
        panic!(
            "SupplyCapExceeded: {} would exceed the cap of {}",
            new_supply, cap
        );
    }
    e.storage()
        .instance()
        .set(&DataKey::TotalSupply, &new_supply);
}

/// Removes `amount` from total supply.
///
/// The mirror of [`increase_supply`]: supply only ever shrinks through burns
/// and clawbacks, so the counter and the sum of balances cannot drift apart.
/// The cap is not re-checked — shrinking supply can never violate it.
///
/// # Panics
///
/// Panics with `SupplyUnderflow` when `amount` exceeds the current supply,
/// which would mean burning tokens that were never minted.
pub fn decrease_supply(e: &Env, amount: i128) {
    let new_supply = total_supply(e).checked_sub(amount).unwrap_or_else(|| {
        panic!(
            "SupplyUnderflow: burning {} would drive supply below zero",
            amount
        )
    });
    e.storage()
        .persistent()
        .set(&DataKey::TotalSupply, &new_supply);
}

/// Brings `amount` of new tokens into circulation for `to`.
///
/// This is the only place that credits balances and grows supply together, so
/// the two ledgers cannot be updated independently and drift apart.
///
/// # Panics
///
/// Panics through [`require_positive_amount`] on a non-positive `amount` and
/// through [`increase_supply`] when the cap would be exceeded.


/// Destroys `amount` of `from`'s tokens, taking them out of circulation.
///
/// The debit runs before the supply shrinks so an insufficient balance aborts
/// before the supply counter moves, and the amount is validated first.
pub fn burn(e: &Env, from: &Address, amount: i128) {
    require_positive_amount(amount);
    debit(e, from, amount);
    decrease_supply(e, amount);
    Burn {
        from: from.clone(),
        amount,
    }
    .publish(e);
}

/// Recovers `amount` of `from`'s tokens for the admin, taking them out of
/// circulation.
///
/// Clawback and burn are deliberately distinct operations wearing similar
/// mechanics: an admin recovery and a holder's own burn both reduce supply,
/// and an auditor reading the event log must be able to tell which happened.
/// Authorization — including the co-signer, when one is set — is the caller's
/// to enforce; this function moves tokens, it does not decide who may ask.
///
/// # Panics
///
/// Panics on a non-positive `amount`, when `from` holds less than `amount`,
/// and through [`decrease_supply`] when the supply counter would go negative.
pub fn clawback(e: &Env, admin: &Address, from: &Address, amount: i128) {
    require_positive_amount(amount);
    debit(e, from, amount);
    decrease_supply(e, amount);
    Clawback {
        admin: admin.clone(),
        from: from.clone(),
        amount,
    }
    .publish(e);
}

/// Moves `amount` of tokens from `from` to `to`, leaving total supply untouched.
///
/// The debit runs before the credit so an insufficient balance aborts before the
/// recipient is paid, and the amount is validated before any balance is read.
///
/// # Panics
///
/// Panics on a non-positive `amount` or when `from` holds less than `amount`.
pub fn transfer(e: &Env, from: &Address, to: &Address, amount: i128) {
    require_positive_amount(amount);
    crate::control::require_not_paused(e);
    crate::control::require_not_frozen(e, from);
    crate::control::require_not_frozen(e, to);
    crate::authorization::require_authorized(e, from);
    crate::authorization::require_authorized(e, to);
    crate::whitelist::require_whitelisted(e, from);
    crate::whitelist::require_whitelisted(e, to);
    debit(e, from, amount);
    credit(e, to, amount);
    Transfer {
        from: from.clone(),
        to: to.clone(),
        amount,
    }
    .publish(e);
}

/// Moves `amount` of tokens from `from` to `to`, carrying `memo`.
///
/// Identical to [`transfer`] in every balance and validation rule; the only
/// difference is the event, which reuses the standard `transfer` topics and
/// appends the memo to the data. A memo over [`events::MAX_MEMO_BYTES`] is
/// refused before anything moves: the memo is paid for by every reader of the
/// ledger, so its size is capped rather than left to the caller.
///
/// # Panics
///
/// Panics on a non-positive `amount`, on a memo over the limit, and when
/// `from` holds less than `amount`.
pub fn transfer_with_memo(e: &Env, from: &Address, to: &Address, amount: i128, memo: &Bytes) {
    require_positive_amount(amount);
    if memo.len() > events::MAX_MEMO_BYTES {
        panic!(
            "MemoTooLarge: {} bytes, {} allowed",
            memo.len(),
            events::MAX_MEMO_BYTES
        );
    }
    debit(e, from, amount);
    credit(e, to, amount);
    events::publish_transfer_with_memo(e, from, to, amount, memo);
}
