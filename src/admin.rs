use crate::storage_types::DataKey;
use soroban_sdk::{Address, Env};

/// True once `initialize` has stored an admin address.
///
/// The admin lives in instance storage: it is fixed configuration, it is
/// restored as a unit with the contract, and an archived admin key would mean
/// a contract that cannot answer who may act at all. See `docs/ttl.md`.
pub fn is_initialized(e: &Env) -> bool {
    e.storage().instance().has(&DataKey::Admin)
}

/// Panics while the contract is still uninitialized.
pub fn require_initialized(e: &Env) {
    if !is_initialized(e) {
        panic!("NotInitialized: call initialize before using the contract");
    }
}

/// The address that currently holds admin authority.
pub fn admin(e: &Env) -> Address {
    e.storage()
        .instance()
        .get(&DataKey::Admin)
        .expect("NotInitialized: admin not set")
}

/// Stores the admin address. Callers must have verified authorization first.
pub fn set_admin(e: &Env, admin_addr: &Address) {
    e.storage().instance().set(&DataKey::Admin, admin_addr);
}

/// True when `caller` is the stored admin.
pub fn is_admin(e: &Env, caller: &Address) -> bool {
    e.storage().instance().get::<_, Address>(&DataKey::Admin) == Some(caller.clone())
}

/// Requires that `caller` is the stored admin and authorized this invocation.
///
/// Admin-gated entry points call this first so an unauthorized address can never
/// reach the state transition behind it.
pub fn check_admin(e: &Env, caller: &Address) {
    require_initialized(e);
    if !is_admin(e, caller) {
        panic!("Unauthorized: caller is not the contract admin");
    }
    caller.require_auth();
}

/// Stores the ledger the contract was initialized on.
pub fn set_initialized_at(e: &Env, ledger: u32) {
    e.storage()
        .instance()
        .set(&DataKey::InitializedAtLedger, &ledger);
}

/// Ledger the contract was initialized on, or 0 when it never was.
pub fn initialized_at_ledger(e: &Env) -> u32 {
    e.storage()
        .instance()
        .get(&DataKey::InitializedAtLedger)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Two-step ownership rotation
// ---------------------------------------------------------------------------

/// Nominates `new_admin` as the next admin, without changing who holds the
/// role.
///
/// The two steps exist because a one-step transfer to a mistyped address is
/// unrecoverable: the admin key is the root of trust for every privileged
/// entry point, so the proposal is recorded and the nominee must accept before
/// authority moves. Until then the current admin keeps full control and can
/// nominate again.
///
/// # Panics
///
/// Panics when the contract is uninitialized, when `new_admin` is the current
/// admin — a rotation to yourself is a no-op that would leave a stale
/// nomination behind — and through [`check_admin`] when the caller is not the
/// admin or did not authorize the call.
pub fn transfer_ownership(e: &Env, new_admin: &Address) {
    let current = admin(e);
    if new_admin == &current {
        panic!("Unauthorized: cannot transfer ownership to the current admin");
    }
    check_admin(e, &current);
    e.storage()
        .persistent()
        .set(&DataKey::PendingAdmin, new_admin);
    e.storage()
        .persistent()
        .set(&DataKey::AdminActiveAfterLedger, &e.ledger().sequence());
}

/// Completes a pending rotation: the nominated address takes the admin role.
///
/// Only the nominee may accept, and it must authorize the call itself — a
/// nomination is an offer, not a power of attorney. On acceptance the previous
/// admin loses authority immediately: there is no overlap window in which two
/// addresses both believe they are admin. The proposal is cleared so a
/// completed transfer cannot be replayed.
///
/// # Panics
///
/// Panics when the contract is uninitialized, when no rotation is pending,
/// when `caller` is not the pending admin, and when `caller` did not authorize
/// the call.
pub fn accept_admin(e: &Env, caller: &Address) {
    require_initialized(e);
    let pending: Address = e
        .storage()
        .persistent()
        .get(&DataKey::PendingAdmin)
        .expect("Unauthorized: no pending admin transfer");
    if pending != *caller {
        panic!("Unauthorized: caller is not the proposed admin");
    }
    caller.require_auth();
    set_admin(e, caller);
    e.storage().persistent().remove(&DataKey::PendingAdmin);
}

/// The nominated admin awaiting acceptance, if a rotation is in flight.
///
/// An operator with no admin key can still see that a rotation is pending and
/// to whom — the question a pending transfer raises for everyone else.
pub fn pending_admin(e: &Env) -> Option<Address> {
    e.storage().persistent().get(&DataKey::PendingAdmin)
}

/// The ledger from which the pending nomination became (or becomes) admin, or
/// 0 when no rotation has ever been proposed.
pub fn admin_active_after_ledger(e: &Env) -> u32 {
    e.storage()
        .persistent()
        .get(&DataKey::AdminActiveAfterLedger)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Clawback co-signer
// ---------------------------------------------------------------------------

/// The second signer required alongside the admin for a clawback, if any.
pub fn read_clawback_cosigner(e: &Env) -> Option<Address> {
    e.storage().persistent().get(&DataKey::ClawbackCosigner)
}

/// Appoints `cosigner` as the second required signer for clawbacks.
///
/// Dual authorization raises the bar for the one operation that confiscates
/// tokens to two independent keys. The co-signer is deliberately rejected when
/// it equals the admin: a co-signer that shares the admin's key satisfies the
/// two-key bar with one key and provides no assurance at all.
///
/// # Panics
///
/// Panics when `cosigner` is the admin, and through [`check_admin`] when the
/// caller is not the admin or did not authorize the call.
pub fn set_clawback_cosigner(e: &Env, admin_addr: &Address, cosigner: &Address) {
    check_admin(e, admin_addr);
    if cosigner == admin_addr {
        panic!(
            "InvalidCosigner: co-signer must differ from the admin, both keys are meant to be independent"
        );
    }
    e.storage()
        .persistent()
        .set(&DataKey::ClawbackCosigner, cosigner);
}

/// Requires the admin's authorization for a clawback, plus the co-signer's
/// when one has been configured.
///
/// With no co-signer set the check is the admin's alone — the control is
/// opt-in, so a single-operator deployment is not forced to appoint a second
/// key, but once one is appointed it can veto every clawback simply by not
/// signing.
///
/// # Panics
///
/// Panics through [`check_admin`] when `admin_addr` is not the admin, and
/// propagates a failed authorization from the co-signer when one is set.
pub fn require_clawback_auth(e: &Env, admin_addr: &Address) {
    check_admin(e, admin_addr);
    if let Some(cosigner) = read_clawback_cosigner(e) {
        cosigner.require_auth();
    }
}
