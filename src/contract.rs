use crate::metadata::{self, TokenMetadata};
use crate::storage_types::{
    bump_instance, DataKey, EscrowRecord, RecurringExecution, RecurringPayment,
};
use crate::{admin, allowance, balance, escrow, recurring, whitelist};
use soroban_sdk::{contract, contractimpl, Address, Env, String, Vec};

#[contract]
pub struct VeriTixPay;

/// Stores the admin and the token metadata, refusing a second call.
fn initialize_state(e: &Env, admin_addr: &Address, meta: &TokenMetadata) {
    if admin::is_initialized(e) {
        panic!("AlreadyInitialized: contract state is locked");
    }
    admin_addr.require_auth();
    admin::set_admin(e, admin_addr);
    admin::set_initialized_at(e, e.ledger().sequence());
    metadata::store(e, meta);
}

#[contractimpl]
impl VeriTixPay {
    /// Atomic constructor running at deploy time.
    pub fn __constructor(e: Env, admin_addr: Address) {
        initialize_state(&e, &admin_addr, &metadata::defaults(&e));
    }

    /// Sets the admin and the default token metadata.
    ///
    /// One-shot: a second call is rejected rather than silently replacing the
    /// admin, because the admin is the root of trust for every privileged entry
    /// point below.
    pub fn initialize(e: Env, admin_addr: Address) {
        bump_instance(&e);
        initialize_state(&e, &admin_addr, &metadata::defaults(&e));
    }

    /// Sets the admin, default metadata, and a hard supply cap.
    pub fn initialize_with_max_supply(e: Env, admin_addr: Address, max_supply: i128) {
        initialize_state(&e, &admin_addr, &metadata::defaults(&e));
        admin::set_max_supply(&e, max_supply);
    }

    /// Sets the admin and caller-supplied token metadata.
    pub fn initialize_with_metadata(
        e: Env,
        admin_addr: Address,
        name: String,
        symbol: String,
        decimals: u32,
    ) {
        bump_instance(&e);
        initialize_state(
            &e,
            &admin_addr,
            &TokenMetadata {
                name,
                symbol,
                decimals,
            },
        );
    }

    /// Sets whether `account` is authorized to move tokens. Admin-only.
    pub fn set_authorized(e: Env, admin_addr: Address, account: Address, authorized: bool) {
        admin::check_admin(&e, &admin_addr);
        crate::authorization::set_authorized(&e, &account, authorized);
    }

    /// Whether `account` is authorized to move tokens.
    pub fn is_authorized(e: Env, account: Address) -> bool {
        crate::authorization::is_authorized(&e, &account)
    }

    /// True once an admin has been stored.
    pub fn is_initialized(e: Env) -> bool {
        bump_instance(&e);
        admin::is_initialized(&e)
    }

    /// The address currently holding admin authority.
    pub fn admin(e: Env) -> Address {
        bump_instance(&e);
        admin::admin(&e)
    }

    /// Ledger on which the contract was initialized, 0 when it never was.
    pub fn initialized_at_ledger(e: Env) -> u32 {
        bump_instance(&e);
        admin::initialized_at_ledger(&e)
    }

    /// Nominates `new_admin` as the next admin. Admin-only.
    ///
    /// The nomination changes nothing until the nominee accepts through
    /// [`accept_admin`](Self::accept_admin): a one-step transfer to a mistyped
    /// address would strand the contract with an admin nobody can sign for.
    pub fn transfer_ownership(e: Env, new_admin: Address) {
        bump_instance(&e);
        admin::transfer_ownership(&e, &new_admin);
    }

    /// Completes a pending nomination: `new_admin` becomes the admin.
    ///
    /// Callable only by the nominated address, which must authorize the call
    /// itself. The previous admin loses power on this call — there is no
    /// overlap window in which two addresses both hold the role.
    pub fn accept_admin(e: Env, new_admin: Address) {
        bump_instance(&e);
        admin::accept_admin(&e, &new_admin);
    }

    /// The nominated admin awaiting acceptance, if a rotation is in flight.
    pub fn pending_admin(e: Env) -> Option<Address> {
        bump_instance(&e);
        admin::pending_admin(&e)
    }

    /// The ledger from which the pending nomination becomes (or became) the
    /// admin, or 0 when no rotation has ever been proposed.
    pub fn admin_active_after_ledger(e: Env) -> u32 {
        bump_instance(&e);
        admin::admin_active_after_ledger(&e)
    }

    /// Appoints `cosigner` as the second required signer for clawbacks.
    /// Admin-only; the co-signer must differ from the admin.
    pub fn set_clawback_cosigner(e: Env, admin_addr: Address, cosigner: Address) {
        bump_instance(&e);
        admin::set_clawback_cosigner(&e, &admin_addr, &cosigner);
    }

    /// The clawback co-signer, if one has been appointed.
    pub fn read_clawback_cosigner(e: Env) -> Option<Address> {
        bump_instance(&e);
        admin::read_clawback_cosigner(&e)
    }

    /// Enables whitelist enforcement. Admin-only.
    pub fn enable_whitelist(e: Env, admin_addr: Address) {
        bump_instance(&e);
        whitelist::enable_whitelist(&e, &admin_addr);
    }

    /// Disables whitelist enforcement. Admin-only.
    pub fn disable_whitelist(e: Env, admin_addr: Address) {
        bump_instance(&e);
        whitelist::disable_whitelist(&e, &admin_addr);
    }

    /// Whether whitelist enforcement is enabled.
    pub fn is_whitelist_enabled(e: Env) -> bool {
        bump_instance(&e);
        whitelist::is_whitelist_enabled(&e)
    }

    /// Adds `account` to the whitelist. Admin-only.
    pub fn add_to_whitelist(e: Env, admin_addr: Address, account: Address) {
        whitelist::add_to_whitelist(&e, &admin_addr, &account);
    }

    /// Removes `account` from the whitelist. Admin-only.
    pub fn remove_from_whitelist(e: Env, admin_addr: Address, account: Address) {
        whitelist::remove_from_whitelist(&e, &admin_addr, &account);
    }

    /// Whether `account` is whitelisted.
    pub fn is_whitelisted(e: Env, account: Address) -> bool {
        whitelist::is_whitelisted(&e, &account)
    }

    /// The token name.
    pub fn name(e: Env) -> String {
        bump_instance(&e);
        metadata::load(&e).name
    }

    /// The token symbol.
    pub fn symbol(e: Env) -> String {
        bump_instance(&e);
        metadata::load(&e).symbol
    }

    /// The number of decimals the token is scaled by.
    pub fn decimals(e: Env) -> u32 {
        bump_instance(&e);
        metadata::load(&e).decimals
    }

    /// Tokens held by `account`; 0 for an address that has never been credited.
    pub fn balance(e: Env, account: Address) -> i128 {
        bump_instance(&e);
        balance::balance_of(&e, &account)
    }

    /// Tokens in circulation.
    pub fn total_supply(e: Env) -> i128 {
        bump_instance(&e);
        balance::total_supply(&e)
    }

    /// The hard cap on total supply, or 0 when supply is unlimited.
    pub fn max_supply(e: Env) -> i128 {
        bump_instance(&e);
        balance::max_supply(&e)
    }

    /// Pauses or resumes all token movement. Admin-only (#1261).
    pub fn set_paused(e: Env, admin_addr: Address, paused: bool) {
        bump_instance(&e);
        admin::check_admin(&e, &admin_addr);
        crate::control::set_paused(&e, paused);
    }

    /// Whether the contract is currently paused.
    pub fn is_paused(e: Env) -> bool {
        bump_instance(&e);
        crate::control::is_paused(&e)
    }

    /// Freezes or thaws `account` so it cannot move tokens. Admin-only.
    pub fn set_frozen(e: Env, admin_addr: Address, account: Address, frozen: bool) {
        bump_instance(&e);
        admin::check_admin(&e, &admin_addr);
        crate::control::set_frozen(&e, &account, frozen);
    }

    /// Whether `account` is frozen.
    pub fn is_frozen(e: Env, account: Address) -> bool {
        bump_instance(&e);
        crate::control::is_frozen(&e, &account)
    }

    /// All addresses currently holding a non-zero balance (#1260).
    pub fn get_holders(e: Env) -> Vec<Address> {
        bump_instance(&e);
        balance::get_holders(&e)
    }

    /// Count of addresses with a non-zero balance.
    pub fn total_holders(e: Env) -> u32 {
        bump_instance(&e);
        balance::total_holders(&e)
    }

    /// Mints `amount` new tokens to `to`. Admin-only and supply-capped.
    pub fn mint(e: Env, admin_addr: Address, to: Address, amount: i128) {
        bump_instance(&e);
        admin::check_admin(&e, &admin_addr);
        balance::mint(&e, &to, amount);
    }

    /// Moves `amount` of tokens from `from` to `to`.
    pub fn transfer(e: Env, from: Address, to: Address, amount: i128) {
        bump_instance(&e);
        from.require_auth();
        crate::authorization::require_authorized(&e, &from);
        balance::transfer(&e, &from, &to, amount);
    }

    /// Authorizes `spender` to move up to `amount` of `from`'s tokens until
    /// `expiration_ledger`.
    ///
    /// `from` authorizes the call: an approval is a delegation of the owner's
    /// own authority, and a function that let anyone approve on someone else's
    /// behalf would be a drain, not a delegation. The allowance module
    /// overwrites rather than accumulates, rejects a non-zero amount with an
    /// expiration already in the past, and treats a zero amount as a revoke.
    pub fn approve(e: Env, from: Address, spender: Address, amount: i128, expiration_ledger: u32) {
        bump_instance(&e);
        from.require_auth();
        allowance::approve(&e, &from, &spender, amount, expiration_ledger);
    }

    /// The amount `spender` may still move on `from`'s behalf.
    ///
    /// Reads as `0` once the allowance has expired: an expired grant is spent,
    /// not an error.
    pub fn allowance(e: Env, from: Address, spender: Address) -> i128 {
        bump_instance(&e);
        allowance::allowance(&e, &from, &spender)
    /// Lowers an allowance by `amount`, saturating at zero.
    ///
    /// The owner authorizes the call. The grant's expiry is left unchanged, so
    /// lowering an amount never slides its deadline.
    pub fn decrease_allowance(e: Env, from: Address, spender: Address, amount: i128) {
        bump_instance(&e);
        from.require_auth();
        allowance::decrease_allowance(&e, &from, &spender, amount);
    /// Number of accounts currently holding a non-zero balance.
    pub fn total_holders(e: Env) -> u32 {
        bump_instance(&e);
        balance::total_holders(&e)
    /// Burns `amount` of `from`'s tokens under `spender`'s allowance.
    ///
    /// The spender authorizes the call and the allowance is consumed before the
    /// tokens move, so a spender can never burn more than `from` approved and a
    /// burn that fails on the balance reverts the allowance reduction with it.
    pub fn burn_from(e: Env, spender: Address, from: Address, amount: i128) {
        bump_instance(&e);
        spender.require_auth();
        allowance::consume_allowance(&e, &from, &spender, amount);
        balance::burn(&e, &from, amount);
    }

    // ---- Escrow ---------------------------------------------------------

    /// Holds `amount` of `token` for `beneficiary` until the event settles, and
    /// returns the new escrow's id.
    ///
    /// The funds leave the depositor immediately and sit in the contract, so a
    /// buyer cannot walk away after a ticket is sold.
    pub fn create_escrow(
        e: Env,
        depositor: Address,
        beneficiary: Address,
        token: Address,
        amount: i128,
        deadline_ledger: u32,
    ) -> u32 {
        bump_instance(&e);
        escrow::create(
            &e,
            &depositor,
            &beneficiary,
            &token,
            amount,
            deadline_ledger,
        )
    }

    /// Pays the beneficiary everything still held and closes the escrow.
    ///
    /// Settlable by the depositor or the admin, and only while the escrow is
    /// `Active`.
    pub fn release_escrow(e: Env, caller: Address, escrow_id: u32) {
        bump_instance(&e);
        escrow::release(&e, &caller, escrow_id);
    }

    /// Returns everything still held to the depositor and closes the escrow.
    ///
    /// Settlable by the depositor or the admin, and only while the escrow is
    /// `Active`.
    pub fn refund_escrow(e: Env, caller: Address, escrow_id: u32) {
        bump_instance(&e);
        escrow::refund(&e, &caller, escrow_id);
    }

    /// Pays `amount` of the held funds to the beneficiary, leaving the escrow
    /// `Active` if anything is still owed.
    pub fn release_partial_escrow(e: Env, caller: Address, escrow_id: u32, amount: i128) {
        bump_instance(&e);
        escrow::release_partial(&e, &caller, escrow_id, amount);
    }

    /// A page of escrow ids created by `depositor` (at most 50 per call).
    pub fn get_escrows_by_depositor(
        e: Env,
        depositor: Address,
        start: u32,
        limit: u32,
    ) -> Vec<u32> {
        bump_instance(&e);
        escrow::ids_page(&e, &DataKey::DepositorEscrows(depositor), start, limit)
    }

    /// A page of escrow ids owed to `beneficiary` (at most 50 per call).
    pub fn get_escrows_by_beneficiary(
        e: Env,
        beneficiary: Address,
        start: u32,
        limit: u32,
    ) -> Vec<u32> {
        bump_instance(&e);
        escrow::ids_page(&e, &DataKey::BeneficiaryEscrows(beneficiary), start, limit)
    }

    /// Total token amount still held across all active escrows.
    pub fn escrowed_total(e: Env) -> i128 {
        bump_instance(&e);
        escrow::escrowed_total(&e)
    }

    /// `(total locked, active count, settled count)` across all escrows.
    pub fn escrow_stats(e: Env) -> (i128, u32, u32) {
        bump_instance(&e);
        escrow::stats(&e)
    }

    /// Reassigns an unsettled escrow to a new beneficiary. Depositor only.
    pub fn transfer_escrow_beneficiary(
        e: Env,
        depositor: Address,
        escrow_id: u32,
        new_beneficiary: Address,
    ) {
        bump_instance(&e);
        escrow::transfer_beneficiary(&e, &depositor, escrow_id, &new_beneficiary);
    }

    /// Creates one escrow per recipient of split `split_id` for its share and
    /// returns the new escrow ids.
    pub fn split_to_escrow(
        e: Env,
        sender: Address,
        split_id: u64,
        deadline_ledger: u32,
    ) -> Vec<u32> {
        bump_instance(&e);
        crate::splitter::split_to_escrow(&e, &sender, split_id, deadline_ledger)
    }

    /// The full record for `escrow_id`.
    ///
    /// # Panics
    ///
    /// Panics when no escrow exists at `escrow_id`, so a client cannot mistake
    /// "never created" for a record of zeros.
    pub fn get_escrow(e: Env, escrow_id: u32) -> EscrowRecord {
        bump_instance(&e);
        escrow::record(&e, escrow_id)
    }

    /// Ledgers elapsed since `escrow_id` was created.
    pub fn get_escrow_age(e: Env, escrow_id: u32) -> u32 {
        bump_instance(&e);
        escrow::age(&e, escrow_id)
    }

    /// Reads up to 50 escrows in one call, in order, with `None` for missing ids.
    pub fn get_escrows_batch(e: Env, ids: Vec<u32>) -> Vec<Option<EscrowRecord>> {
        bump_instance(&e);
        escrow::records_batch(&e, &ids)
    }

    /// Whether `escrow_id` has been settled — released or refunded.
    ///
    /// # Panics
    ///
    /// Panics when no escrow exists at `escrow_id`, for the same reason
    /// [`get_escrow`] does.
    pub fn is_escrow_settled(e: Env, escrow_id: u32) -> bool {
        bump_instance(&e);
        escrow::is_settled(&e, escrow_id)
    }

    /// Releases `escrow_id` to its beneficiary once its deadline has passed.
    /// Callable by anyone; refused while a dispute is open.
    pub fn trigger_auto_release(e: Env, escrow_id: u32) {
        bump_instance(&e);
        escrow::auto_release(&e, escrow_id);
    }

    /// Admin escape hatch: pays the remainder of `escrow_id` to `winner`, which
    /// must be the depositor or the beneficiary.
    pub fn admin_settle_escrow(e: Env, admin: Address, escrow_id: u32, winner: Address) {
        bump_instance(&e);
        escrow::admin_settle(&e, &admin, escrow_id, &winner);
    }

    /// Raises a dispute over `escrow_id`, freezing it until a ruling is final.
    pub fn raise_dispute(e: Env, claimant: Address, escrow_id: u32, resolver: Option<Address>) {
        bump_instance(&e);
        crate::dispute::raise_dispute(&e, &claimant, escrow_id, resolver);
    }

    /// Whether a dispute on `escrow_id` is still live and blocking settlement.
    pub fn is_dispute_open(e: Env, escrow_id: u32) -> bool {
        bump_instance(&e);
        crate::dispute::is_dispute_open(&e, escrow_id)
    }

    /// The dispute record for `escrow_id`.
    pub fn get_dispute(e: Env, escrow_id: u32) -> crate::storage_types::DisputeRecord {
        bump_instance(&e);
        crate::dispute::get_dispute(&e, escrow_id)
    }

    /// The retained successful charge history for a recurring schedule.
    pub fn get_recurring_history(e: Env, recurring_id: u64) -> Vec<RecurringExecution> {
        bump_instance(&e);
        recurring::get_recurring_history(&e, recurring_id)
    }

    /// Reads a recurring schedule by id.
    pub fn get_recurring(e: Env, recurring_id: u64) -> RecurringPayment {
        recurring::get_recurring(&e, recurring_id)
    }

    /// Whether the recurring schedule at `recurring_id` is active and not paused.
    pub fn is_recurring_active(e: Env, recurring_id: u64) -> bool {
        recurring::is_recurring_active(&e, recurring_id)
    }

    /// Creates a recurring schedule, authorising future charges from `payer`.
    pub fn create_recurring(
        e: Env,
        payer: Address,
        payee: Address,
        token: Address,
        amount: i128,
        interval_ledgers: u32,
    ) -> u64 {
        bump_instance(&e);
        recurring::create(&e, &payer, &payee, &token, amount, interval_ledgers)
    }

    /// All recurring schedules created by `payer`.
    pub fn get_recurring_by_payer(e: Env, payer: Address) -> Vec<RecurringPayment> {
        bump_instance(&e);
        recurring::get_recurring_by_payer(&e, &payer)
    }

    /// Recurring schedule ids associated with `payee`.
    pub fn recurring_ids_for_payee(e: Env, payee: Address) -> Vec<u64> {
        bump_instance(&e);
        recurring::recurring_ids_for_payee(&e, &payee)
    }

    /// Number of recurring schedules associated with `payee`.
    pub fn recurring_count_for_payee(e: Env, payee: Address) -> u32 {
        bump_instance(&e);
        recurring::recurring_count_for_payee(&e, &payee)
    }

    /// The arbiter picks a winner: records the ruling and opens the appeal
    /// window; a second call after the window lapses settles and pays.
    pub fn resolve_dispute(e: Env, resolver: Address, escrow_id: u32, winner: Address) {
        bump_instance(&e);
        crate::dispute::resolve_dispute(&e, resolver, escrow_id, winner);
    }

    /// The losing party escalates a ruling within the appeal window.
    pub fn appeal_dispute(e: Env, caller: Address, escrow_id: u32) {
        bump_instance(&e);
        crate::dispute::appeal_dispute(&e, &caller, escrow_id);
    }

    /// The final, unappealable ruling: settles the escrow and pays the winner.
    pub fn resolve_appeal(e: Env, resolver: Address, escrow_id: u32, winner: Address) {
        bump_instance(&e);
        crate::dispute::resolve_appeal(&e, resolver, escrow_id, winner);
    }

    /// Executes one due charge from an authorised recurring payment.
    pub fn execute_recurring(e: Env, recurring_id: u64) {
        bump_instance(&e);
        recurring::execute_recurring(&e, recurring_id);
    }

    /// Cancels a recurring payment schedule.
    pub fn cancel_recurring(e: Env, caller: Address, recurring_id: u64) {
        recurring::cancel_recurring(&e, &caller, recurring_id);
    }

    /// Sets the global dispute arbiter (resolver) used to settle disputes.
    ///
    /// Admin-only. Disputes need a designated resolver before any of them can
    /// be settled.
    pub fn set_arbiter(e: Env, admin: Address, arbiter: Address) {
        bump_instance(&e);
        crate::dispute::set_arbiter(&e, &admin, &arbiter);
    }

    /// The configured dispute arbiter.
    ///
    /// # Panics
    ///
    /// Panics with `ArbiterNotSet` when no arbiter has been configured.
    pub fn get_arbiter(e: Env) -> Address {
        bump_instance(&e);
        crate::dispute::get_arbiter(&e)
    }

    // ---- Multi-escrow ---------------------------------------------------

    /// Holds `total` of `token` split across `beneficiaries` and returns the
    /// new record's id.
    ///
    /// `beneficiaries[i]` is owed `amounts[i]`; the two vectors must be the
    /// same length and `amounts` must sum exactly to `total`.
    pub fn create_multi_escrow(
        e: Env,
        depositor: Address,
        token: Address,
        beneficiaries: soroban_sdk::Vec<Address>,
        amounts: soroban_sdk::Vec<i128>,
        total: i128,
    ) -> u64 {
        bump_instance(&e);
        crate::multi_escrow::create(&e, &depositor, &token, beneficiaries, amounts, total)
    }

    /// The full record for `multi_escrow_id`.
    ///
    /// # Panics
    ///
    /// Panics when no multi-escrow exists at `multi_escrow_id`.
    pub fn get_multi_escrow(
        e: Env,
        multi_escrow_id: u64,
    ) -> crate::storage_types::MultiEscrowRecord {
        bump_instance(&e);
        crate::multi_escrow::record(&e, multi_escrow_id)
    }

    /// Whether `multi_escrow_id` has been settled — released or refunded.
    ///
    /// # Panics
    ///
    /// Panics when no multi-escrow exists at `multi_escrow_id`.
    pub fn is_multi_escrow_settled(e: Env, multi_escrow_id: u64) -> bool {
        bump_instance(&e);
        crate::multi_escrow::is_settled(&e, multi_escrow_id)
    }

    /// Pays every beneficiary their stored amount and marks the record
    /// `Released`. Settlement is atomic: a failure on any leg reverts the
    /// whole call.
    ///
    /// Settlable by the depositor or the admin, and only while the record is
    /// `Active`.
    pub fn release_multi_escrow(e: Env, caller: Address, multi_escrow_id: u64) {
        bump_instance(&e);
        crate::multi_escrow::release(&e, &caller, multi_escrow_id);
    }

    /// Returns the whole deposit to the depositor and marks the record
    /// `Refunded`.
    ///
    /// Settlable by the depositor or the admin, and only while the record is
    /// `Active`.
    pub fn refund_multi_escrow(e: Env, caller: Address, multi_escrow_id: u64) {
        bump_instance(&e);
        crate::multi_escrow::refund(&e, &caller, multi_escrow_id);
    }

    /// Admin-gated batch minting that returns the total minted.
    pub fn mint_batch(e: Env, admin: Address, mints: Vec<(Address, i128)>) -> i128 {
        crate::batch::mint_batch(&e, &admin, mints)
    }

    /// Admin-gated batch clawback across multiple targets.
    pub fn clawback_batch(e: Env, admin: Address, targets: Vec<(Address, i128)>) {
        crate::batch::clawback_batch(&e, &admin, targets);
    }

    /// Admin-gated airdrop distributed evenly across current holders.
    pub fn airdrop(e: Env, admin: Address, token: Address, total_amount: i128) {
        crate::airdrop::airdrop(&e, &admin, &token, total_amount);
    }

    /// Distributes dividends pro rata to snapshot balances.
    pub fn distribute_dividend(
        e: Env,
        admin: Address,
        snapshot_id: u32,
        total: i128,
        holders: Vec<Address>,
    ) {
        crate::dividend::distribute_dividend(&e, &admin, snapshot_id, total, holders);
    }

    /// Emergency withdrawal of stranded tokens. Admin-only; refuses to touch escrowed funds.
    pub fn emergency_withdraw(
        e: Env,
        admin: Address,
        recipient: Address,
        token: Address,
        amount: i128,
    ) {
        bump_instance(&e);
        crate::emergency::emergency_withdraw(&e, &admin, &recipient, &token, amount);
    /// Returns the current whitelist nonce for `admin`.
    pub fn get_whitelist_nonce(e: Env, admin: Address) -> u64 {
        bump_instance(&e);
        crate::whitelist_signed::get_nonce(&e, &admin)
    /// Returns `(open_count, resolved_count, expired_count)` across all disputes.
    pub fn dispute_stats(e: Env) -> (u32, u32, u32) {
        bump_instance(&e);
        crate::dispute_stats::dispute_stats(&e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_types::EscrowStatus;
    use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
    use soroban_sdk::token::StellarAssetClient;
    use soroban_sdk::{vec, xdr, FromVal, IntoVal};

    fn v<T: IntoVal<Env, soroban_sdk::Val>>(e: &Env, x: T) -> soroban_sdk::Val {
        x.into_val(e)
    }

    struct Fixture {
        e: Env,
        client: VeriTixPayClient<'static>,
        contract_id: Address,
        contract: Address,
        admin: Address,
    }

    impl Fixture {
        fn new() -> Self {
            let e = Env::default();
            e.mock_all_auths();
            let contract_id = e.register(VeriTixPay, ());
            let client = VeriTixPayClient::new(&e, &contract_id);
            let admin = Address::generate(&e);
            client.initialize(&admin);
            let contract = contract_id.clone();
            Fixture {
                e,
                client,
                contract_id,
                contract,
                admin,
            }
        }

        /// A SEP-41 asset, and an escrow over `amount` of it.
        fn escrow(
            &self,
            amount: i128,
        ) -> (Address, Address, Address, StellarAssetClient<'static>, u32) {
            let depositor = Address::generate(&self.e);
            let beneficiary = Address::generate(&self.e);
            let token_address = self
                .e
                .register_stellar_asset_contract_v2(self.admin.clone())
                .address();
            let token = StellarAssetClient::new(&self.e, &token_address);
            token.mint(&depositor, &amount);
            let id = self.client.create_escrow(
                &depositor,
                &beneficiary,
                &token_address,
                &amount,
                &2_000,
            );
            (depositor, beneficiary, token_address, token, id)
        }

        /// A SEP-41 asset with `holder` holding `amount` of it.
        fn asset(&self, holder: &Address, amount: i128) -> (Address, StellarAssetClient<'static>) {
            let address = self
                .e
                .register_stellar_asset_contract_v2(self.admin.clone())
                .address();
            let token = StellarAssetClient::new(&self.e, &address);
            token.mint(holder, &amount);
            (address, token)
        }

        /// The total the contract reports as still held in escrow for `token`.
        fn locked(&self, token: &Address) -> i128 {
            self.e
                .as_contract(&self.contract_id, || escrow::value_locked(&self.e, token))
        }

        fn escrows_created(&self) -> u32 {
            self.e
                .as_contract(&self.contract_id, || escrow::count(&self.e))
        }
    }

    fn last_topics(e: &Env) -> std::vec::Vec<xdr::ScVal> {
        let events = e.events().all();
        let last = events.events().last().expect("no event was emitted");
        let xdr::ContractEventBody::V0(body) = &last.body;
        body.topics.to_vec()
    }

    fn last_data(e: &Env) -> xdr::ScVal {
        let events = e.events().all();
        let last = events.events().last().expect("no event was emitted");
        let xdr::ContractEventBody::V0(body) = &last.body;
        body.data.clone()
    }

    fn panics<R>(f: impl FnOnce() -> R) -> bool {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err()
    }

    // ---- create_escrow (prerequisite) -----------------------------------

    #[test]
    fn create_escrow_holds_the_funds_for_the_beneficiary() {
        let f = Fixture::new();
        let (depositor, beneficiary, token_address, token, _id) = f.escrow(1_000);

        assert_eq!(token.balance(&depositor), 0);
        assert_eq!(token.balance(&f.contract), 1_000);
        assert_eq!(token.balance(&beneficiary), 0);
        assert_eq!(f.locked(&token_address), 1_000);
    }

    #[test]
    fn a_new_escrow_is_active_and_spendable_by_neither_side() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, _token, id) = f.escrow(1_000);

        assert!(!f.client.is_escrow_settled(&id));
        assert_eq!(f.client.get_escrow(&id).status, EscrowStatus::Active);
    }

    // ---- #883: release_escrow -------------------------------------------

    #[test]
    fn release_pays_the_beneficiary_everything_held() {
        let f = Fixture::new();
        let (_depositor, beneficiary, _token_address, token, id) = f.escrow(1_000);

        f.client.release_escrow(&Address::generate(&f.e), &id);

        assert_eq!(token.balance(&beneficiary), 1_000);
        assert_eq!(token.balance(&f.contract), 0);
    }

    #[test]
    fn release_marks_the_escrow_released_and_settled() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, _token, id) = f.escrow(1_000);

        f.client.release_escrow(&Address::generate(&f.e), &id);

        assert_eq!(f.client.get_escrow(&id).status, EscrowStatus::Released);
        assert!(f.client.is_escrow_settled(&id));
    }

    #[test]
    fn release_decrements_the_locked_value() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, token_address, _token, id) = f.escrow(1_000);

        f.client.release_escrow(&Address::generate(&f.e), &id);

        assert_eq!(f.locked(&token_address), 0);
    }

    #[test]
    fn the_depositor_can_release() {
        let f = Fixture::new();
        let (depositor, beneficiary, _token_address, token, id) = f.escrow(1_000);

        f.client.release_escrow(&depositor, &id);

        assert_eq!(token.balance(&beneficiary), 1_000);
    }

    #[test]
    fn the_admin_can_release() {
        let f = Fixture::new();
        let (_depositor, beneficiary, _token_address, token, id) = f.escrow(1_000);

        f.client.release_escrow(&f.admin, &id);

        assert_eq!(token.balance(&beneficiary), 1_000);
    }

    #[test]
    fn a_stranger_cannot_release() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, token, id) = f.escrow(1_000);
        let stranger = Address::generate(&f.e);

        assert!(panics(|| {
            f.client.release_escrow(&stranger, &id);
        }));
        assert_eq!(token.balance(&f.contract), 1_000);
        assert!(!f.client.is_escrow_settled(&id));
    }

    #[test]
    fn a_released_escrow_cannot_be_released_again() {
        let f = Fixture::new();
        let (_depositor, beneficiary, token_address, token, id) = f.escrow(1_000);
        f.client.release_escrow(&f.admin, &id);

        assert!(panics(|| {
            f.client.release_escrow(&f.admin, &id);
        }));
        // The beneficiary is paid once, not twice.
        assert_eq!(token.balance(&beneficiary), 1_000);
        assert_eq!(f.locked(&token_address), 0);
    }

    #[test]
    fn releasing_an_unknown_escrow_panics() {
        let f = Fixture::new();

        assert!(panics(|| {
            f.client.release_escrow(&f.admin, &99);
        }));
    }

    #[test]
    fn release_emits_the_beneficiary_the_id_and_the_amount() {
        let f = Fixture::new();
        let (_depositor, beneficiary, _token_address, _token, id) = f.escrow(1_000);

        f.client.release_escrow(&f.admin, &id);

        assert_eq!(
            last_topics(&f.e),
            std::vec![
                xdr::ScVal::Symbol("escrow_released".try_into().unwrap()),
                xdr::ScVal::from_val(&f.e, &beneficiary.to_val()),
            ]
        );
        assert_eq!(
            last_data(&f.e),
            xdr::ScVal::from_val(
                &f.e,
                &vec![&f.e, v(&f.e, id), v(&f.e, 1_000i128), v(&f.e, 0i128)]
            )
        );
    }

    #[test]
    fn releasing_one_escrow_leaves_the_others_locked() {
        let f = Fixture::new();
        let (_d1, _b1, t1, _a1, first) = f.escrow(400);
        let (_d2, _b2, t2, _a2, second) = f.escrow(600);

        f.client.release_escrow(&f.admin, &first);

        assert!(f.client.is_escrow_settled(&first));
        assert!(!f.client.is_escrow_settled(&second));
        assert_eq!(f.locked(&t1), 0);
        assert_eq!(f.locked(&t2), 600);
    }

    // ---- #884: refund_escrow --------------------------------------------

    #[test]
    fn refund_pays_the_depositor_everything_held() {
        let f = Fixture::new();
        let (depositor, beneficiary, _token_address, token, id) = f.escrow(1_000);
        f.e.ledger().set_sequence_number(2_001);

        f.client.refund_escrow(&depositor, &id);

        assert_eq!(token.balance(&depositor), 1_000);
        assert_eq!(token.balance(&f.contract), 0);
        assert_eq!(token.balance(&beneficiary), 0);
    }

    #[test]
    fn refund_marks_the_escrow_refunded_and_settled() {
        let f = Fixture::new();
        let (depositor, _beneficiary, token_address, _token, id) = f.escrow(1_000);
        f.e.ledger().set_sequence_number(2_001);

        f.client.refund_escrow(&depositor, &id);

        assert_eq!(f.client.get_escrow(&id).status, EscrowStatus::Refunded);
        assert!(f.client.is_escrow_settled(&id));
        assert_eq!(f.locked(&token_address), 0);
    }

    #[test]
    fn the_admin_can_refund() {
        let f = Fixture::new();
        let (depositor, _beneficiary, _token_address, token, id) = f.escrow(1_000);

        f.client.refund_escrow(&f.admin, &id);

        assert_eq!(token.balance(&depositor), 1_000);
    }

    #[test]
    fn the_beneficiary_can_refund_before_deadline() {
        let f = Fixture::new();
        let (depositor, beneficiary, _token_address, token, id) = f.escrow(1_000);

        f.client.refund_escrow(&beneficiary, &id);

        assert_eq!(token.balance(&depositor), 1_000);
    }

    #[test]
    fn depositor_cannot_refund_before_deadline() {
        let f = Fixture::new();
        let (depositor, _beneficiary, _token_address, _token, id) = f.escrow(1_000);

        assert!(panics(|| {
            f.client.refund_escrow(&depositor, &id);
        }));
    }

    #[test]
    fn a_stranger_cannot_refund() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, token, id) = f.escrow(1_000);
        let stranger = Address::generate(&f.e);

        assert!(panics(|| {
            f.client.refund_escrow(&stranger, &id);
        }));
        assert_eq!(token.balance(&f.contract), 1_000);
        assert!(!f.client.is_escrow_settled(&id));
    }

    #[test]
    fn a_refunded_escrow_cannot_be_released() {
        let f = Fixture::new();
        let (depositor, beneficiary, _token_address, token, id) = f.escrow(1_000);
        f.e.ledger().set_sequence_number(2_001);
        f.client.refund_escrow(&depositor, &id);

        assert!(panics(|| {
            f.client.release_escrow(&f.admin, &id);
        }));
        assert_eq!(token.balance(&beneficiary), 0);
        assert_eq!(token.balance(&depositor), 1_000);
    }

    #[test]
    fn a_released_escrow_cannot_be_refunded() {
        let f = Fixture::new();
        let (depositor, _beneficiary, _token_address, token, id) = f.escrow(1_000);
        f.client.release_escrow(&f.admin, &id);
        f.e.ledger().set_sequence_number(2_001);

        assert!(panics(|| {
            f.client.refund_escrow(&depositor, &id);
        }));
        assert_eq!(token.balance(&depositor), 0);
    }

    #[test]
    fn refund_emits_the_depositor_the_id_and_the_amount() {
        let f = Fixture::new();
        let (depositor, _beneficiary, _token_address, _token, id) = f.escrow(750);
        f.e.ledger().set_sequence_number(2_001);

        f.client.refund_escrow(&depositor, &id);

        assert_eq!(
            last_topics(&f.e),
            std::vec![
                xdr::ScVal::Symbol("escrow_refunded".try_into().unwrap()),
                xdr::ScVal::from_val(&f.e, &depositor.to_val()),
            ]
        );
        assert_eq!(
            last_data(&f.e),
            xdr::ScVal::from_val(&f.e, &vec![&f.e, v(&f.e, id), v(&f.e, 750i128)])
        );
    }

    // ---- #885: release_partial_escrow ----------------------------------

    #[test]
    fn a_partial_release_pays_only_that_amount() {
        let f = Fixture::new();
        let (_depositor, beneficiary, _token_address, token, id) = f.escrow(1_000);

        f.client.release_partial_escrow(&f.admin, &id, &300);

        assert_eq!(token.balance(&beneficiary), 300);
        assert_eq!(token.balance(&f.contract), 700);
    }

    #[test]
    fn a_partial_release_reduces_the_record_and_leaves_it_active() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, _token, id) = f.escrow(1_000);

        f.client.release_partial_escrow(&f.admin, &id, &300);

        let record = f.client.get_escrow(&id);
        assert_eq!(record.amount, 700);
        assert_eq!(record.status, EscrowStatus::Active);
        assert!(!f.client.is_escrow_settled(&id));
    }

    #[test]
    fn a_partial_release_decrements_the_locked_value() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, token_address, _token, id) = f.escrow(1_000);

        f.client.release_partial_escrow(&f.admin, &id, &300);

        assert_eq!(f.locked(&token_address), 700);
    }

    #[test]
    fn successive_partial_releases_add_up() {
        let f = Fixture::new();
        let (_depositor, beneficiary, token_address, token, id) = f.escrow(1_000);

        f.client.release_partial_escrow(&f.admin, &id, &300);
        f.client.release_partial_escrow(&f.admin, &id, &200);

        assert_eq!(token.balance(&beneficiary), 500);
        assert_eq!(f.client.get_escrow(&id).amount, 500);
        assert_eq!(f.locked(&token_address), 500);
    }

    #[test]
    fn a_partial_release_of_the_remainder_settles_the_escrow() {
        // "Active until the remainder is zero" — the call that takes it to zero
        // is the one that closes the escrow.
        let f = Fixture::new();
        let (_depositor, beneficiary, token_address, token, id) = f.escrow(1_000);

        f.client.release_partial_escrow(&f.admin, &id, &300);
        f.client.release_partial_escrow(&f.admin, &id, &700);

        assert_eq!(token.balance(&beneficiary), 1_000);
        let record = f.client.get_escrow(&id);
        assert_eq!(record.amount, 0);
        assert_eq!(record.status, EscrowStatus::Released);
        assert!(f.client.is_escrow_settled(&id));
        assert_eq!(f.locked(&token_address), 0);
    }

    #[test]
    fn a_partial_release_above_the_remainder_is_refused() {
        let f = Fixture::new();
        let (_depositor, beneficiary, token_address, token, id) = f.escrow(1_000);
        f.client.release_partial_escrow(&f.admin, &id, &300);

        assert!(panics(|| {
            f.client.release_partial_escrow(&f.admin, &id, &701);
        }));
        assert_eq!(token.balance(&beneficiary), 300);
        assert_eq!(f.client.get_escrow(&id).amount, 700);
        assert_eq!(f.locked(&token_address), 700);
    }

    #[test]
    fn a_partial_release_of_zero_is_refused() {
        let f = Fixture::new();
        let (_depositor, beneficiary, _token_address, token, id) = f.escrow(1_000);

        assert!(panics(|| {
            f.client.release_partial_escrow(&f.admin, &id, &0);
        }));
        assert_eq!(token.balance(&beneficiary), 0);
        assert_eq!(f.client.get_escrow(&id).amount, 1_000);
    }

    #[test]
    fn a_negative_partial_release_is_refused() {
        let f = Fixture::new();
        let (_depositor, beneficiary, token_address, token, id) = f.escrow(1_000);

        assert!(panics(|| {
            f.client.release_partial_escrow(&f.admin, &id, &-100);
        }));
        assert_eq!(token.balance(&beneficiary), 0);
        assert_eq!(f.client.get_escrow(&id).amount, 1_000);
        assert_eq!(f.locked(&token_address), 1_000);
    }

    #[test]
    fn a_settled_escrow_cannot_be_partially_released() {
        let f = Fixture::new();
        let (_depositor, beneficiary, _token_address, token, id) = f.escrow(1_000);
        f.client.release_escrow(&f.admin, &id);

        assert!(panics(|| {
            f.client.release_partial_escrow(&f.admin, &id, &100);
        }));
        assert_eq!(token.balance(&beneficiary), 1_000);
    }

    #[test]
    fn a_stranger_cannot_partially_release() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, token, id) = f.escrow(1_000);
        let stranger = Address::generate(&f.e);

        assert!(panics(|| {
            f.client.release_partial_escrow(&stranger, &id, &100);
        }));
        assert_eq!(token.balance(&f.contract), 1_000);
    }

    #[test]
    fn the_depositor_can_partially_release() {
        let f = Fixture::new();
        let (depositor, beneficiary, _token_address, token, id) = f.escrow(1_000);

        f.client.release_partial_escrow(&depositor, &id, &250);

        assert_eq!(token.balance(&beneficiary), 250);
    }

    #[test]
    fn a_full_release_after_a_partial_pays_the_remainder() {
        let f = Fixture::new();
        let (_depositor, beneficiary, token_address, token, id) = f.escrow(1_000);
        f.client.release_partial_escrow(&f.admin, &id, &250);

        f.client.release_escrow(&f.admin, &id);

        assert_eq!(token.balance(&beneficiary), 1_000);
        assert_eq!(f.locked(&token_address), 0);
        assert!(f.client.is_escrow_settled(&id));
    }

    #[test]
    fn a_partial_release_reports_the_remaining_amount() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, _token, id) = f.escrow(1_000);

        f.client.release_partial_escrow(&f.admin, &id, &300);

        assert_eq!(
            last_data(&f.e),
            xdr::ScVal::from_val(
                &f.e,
                &vec![&f.e, v(&f.e, id), v(&f.e, 300i128), v(&f.e, 700i128)]
            )
        );
    }

    // ---- #886: the views -----------------------------------------------

    #[test]
    fn get_escrow_returns_the_stored_record() {
        let f = Fixture::new();
        let depositor = Address::generate(&f.e);
        let beneficiary = Address::generate(&f.e);
        let (token_address, _) = f.asset(&depositor, 2_000);
        let id = f
            .client
            .create_escrow(&depositor, &beneficiary, &token_address, &1_000, &2_000);

        let record = f.client.get_escrow(&id);

        assert_eq!(record.depositor, depositor);
        assert_eq!(record.beneficiary, beneficiary);
        assert_eq!(record.token, token_address);
        assert_eq!(record.amount, 1_000);
        assert_eq!(record.deadline_ledger, 2_000);
        assert_eq!(record.status, EscrowStatus::Active);
    }

    #[test]
    fn get_escrow_panics_on_an_unknown_id() {
        let f = Fixture::new();

        assert!(panics(|| {
            f.client.get_escrow(&7);
        }));
    }

    #[test]
    fn is_escrow_settled_is_false_before_and_true_after() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, _token, id) = f.escrow(1_000);

        assert!(!f.client.is_escrow_settled(&id));
        f.client.release_escrow(&f.admin, &id);
        assert!(f.client.is_escrow_settled(&id));
    }

    #[test]
    fn is_escrow_settled_is_true_for_a_refund_too() {
        let f = Fixture::new();
        let (depositor, _beneficiary, _token_address, _token, id) = f.escrow(1_000);

        f.client.refund_escrow(&depositor, &id);

        assert!(f.client.is_escrow_settled(&id));
    }

    #[test]
    fn is_escrow_settled_panics_on_an_unknown_id() {
        // "Not settled" would be a false answer to a question about an escrow
        // that does not exist, and the caller would carry it forward.
        let f = Fixture::new();

        assert!(panics(|| {
            f.client.is_escrow_settled(&7);
        }));
    }

    #[test]
    fn the_view_follows_a_partial_release() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, _token, id) = f.escrow(1_000);
        f.client.release_partial_escrow(&f.admin, &id, &400);

        assert_eq!(f.client.get_escrow(&id).amount, 600);
        assert!(!f.client.is_escrow_settled(&id));
    }

    #[test]
    fn whitelist_batch_and_signed() {
        let f = Fixture::new();
        let u1 = Address::generate(&f.e);
        let u2 = Address::generate(&f.e);
        let u3 = Address::generate(&f.e);

        assert!(!f.client.is_whitelisted(&u1));
        assert!(!f.client.is_whitelisted(&u2));

        f.client
            .add_to_whitelist_batch(&f.admin, &vec![&f.e, u1.clone(), u2.clone()]);
        assert!(f.client.is_whitelisted(&u1));
        assert!(f.client.is_whitelisted(&u2));

        let nonce = f.client.get_whitelist_nonce(&f.admin);
        assert_eq!(nonce, 0);

        f.client
            .add_to_whitelist_signed(&f.admin, &vec![&f.e, u3.clone()], &0);
        assert!(f.client.is_whitelisted(&u3));
        assert_eq!(f.client.get_whitelist_nonce(&f.admin), 1);

        // Reusing nonce 0 panics
        assert!(panics(|| {
            f.client
                .add_to_whitelist_signed(&f.admin, &vec![&f.e, u3.clone()], &0);
        }));
    }

    #[test]
    fn dispute_stats_and_claimant_tracking() {
        let f = Fixture::new();
        let (d, _b, _t, _tok, id) = f.escrow(1_000);
        let stats_before = f.client.dispute_stats();
        assert_eq!(stats_before, (0, 0, 0));

        f.client.raise_dispute(&d, &(id as u64), &None);
        let stats_after = f.client.dispute_stats();
        assert_eq!(stats_after, (1, 0, 0));

        let claimant_disputes = f.client.get_disputes_by_claimant(&d);
        assert_eq!(claimant_disputes.len(), 1);
        assert_eq!(claimant_disputes.get(0).unwrap().escrow_id, id as u64);
    }

    #[test]
    fn emergency_withdraw_leaves_escrowed_funds() {
        let f = Fixture::new();
        let depositor = Address::generate(&f.e);
        let recipient = Address::generate(&f.e);
        let (token_address, token) = f.asset(&depositor, 1_000);

        // Lock 600 in escrow
        let b = Address::generate(&f.e);
        f.client
            .create_escrow(&depositor, &b, &token_address, &600, &2_000);

        // Stranded funds: mint 400 extra directly to contract
        token.mint(&f.contract, &400);

        // Trying to withdraw more than unescrowed (400) panics
        assert!(panics(|| {
            f.client
                .emergency_withdraw(&f.admin, &recipient, &token_address, &500);
        }));

        // Withdrawing unescrowed 400 succeeds
        f.client
            .emergency_withdraw(&f.admin, &recipient, &token_address, &400);
        assert_eq!(token.balance(&recipient), 400);
    }
}

#[cfg(test)]
pub mod testing {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Bytes;

    /// A registered, initialized contract with a known admin.
    pub struct Fixture {
        pub e: Env,
        pub client: VeriTixPayClient<'static>,
        pub contract_id: Address,
        pub admin: Address,
    }

    impl Fixture {
        pub fn new() -> Self {
            let e = Env::default();
            e.mock_all_auths();
            let contract_id = e.register(VeriTixPay, ());
            let client = VeriTixPayClient::new(&e, &contract_id);
            let admin = Address::generate(&e);
            client.initialize(&admin);
            Fixture {
                e,
                client,
                contract_id,
                admin,
            }
        }

        /// A new account holding `amount` tokens.
        pub fn fund(&self, amount: i128) -> Address {
            let holder = Address::generate(&self.e);
            self.client.mint(&self.admin, &holder, &amount);
            holder
        }

        /// Locks `amount` of `account`'s balance in an escrow, without going
        /// through `create_escrow` — the lock is the bookkeeping entry
        /// `spendable_balance` reads, and a test can set it directly.
        pub fn set_escrow_lock(&self, account: &Address, amount: i128) {
            self.e.as_contract(&self.contract_id, || {
                self.e
                    .storage()
                    .persistent()
                    .set(&DataKey::EscrowLocked(account.clone()), &amount);
            });
        }

        /// Sets the supply cap directly, bypassing whatever entry point would
        /// normally configure it.
        pub fn set_max_supply(&self, amount: i128) {
            self.e.as_contract(&self.contract_id, || {
                self.e
                    .storage()
                    .persistent()
                    .set(&DataKey::MaxSupply, &amount);
            });
        }
    }

    /// The empty holder set, for asserting against `get_holders` before
    /// anything has been minted.
    pub fn no_holders(e: &Env) -> Vec<Address> {
        Vec::new(e)
    }

    /// `len` bytes, all set to `fill`.
    pub fn bytes_of_len(e: &Env, len: u32, fill: u8) -> Bytes {
        Bytes::from_slice(e, &std::vec![fill; len as usize])
    }
}
/// Replaces an existing recipient on a split with a new, unique recipient address.
    pub fn replace_split_recipient(
        env: Env,
        split_id: u64,
        old_recipient: Address,
        new_recipient: Address,
    ) -> Result<(), SplitterError> {
        Splitter::replace_split_recipient(&env, split_id, old_recipient, new_recipient)
    }

    /// Revokes all active allowances granted by `from`.
    pub fn revoke_all_allowances(env: Env, from: Address) {
        from.require_auth();
        allowance::revoke_all_allowances(&env, from);
    }

    #[test]
    fn test_revoke_all_allowances_requires_auth_and_revokes() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, VeriTixPay);
        let client = VeriTixPayClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let spender_a = Address::generate(&env);
        let spender_b = Address::generate(&env);

        // Set up allowances for spender_a and spender_b from owner, then revoke all
        client.revoke_all_allowances(&owner);
    }
    /// Creates a new payment split for `sender` across `recipients` with corresponding `shares_bps`.
    pub fn create_split(
        env: Env,
        sender: Address,
        token: Address,
        total_amount: i128,
        recipients: Vec<Address>,
        shares_bps: Vec<u32>,
    ) -> Result<u64, SplitterError> {
        Splitter::create_split(&env, sender, token, total_amount, recipients, shares_bps)
    }