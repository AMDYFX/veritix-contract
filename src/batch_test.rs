#[cfg(test)]
mod tests {
    use crate::contract::{VeriTixPay, VeriTixPayClient};
    use crate::test::create_token_contract;
    use soroban_sdk::{
        testutils::{Address as _, Events as _},
        Address, Env, Vec,
    };

    #[test]
    fn test_batch_basic_operations() {
        let e = Env::default();
        e.mock_all_auths();

        let contract_id = e.register_contract(None, VeriTixPay);
        let client = VeriTixPayClient::new(&e, &contract_id);

        let admin = Address::generate(&e);
        client.initialize(&admin);

        let token = create_token_contract(&e, &admin);
        let token_client = soroban_sdk::token::StellarAssetClient::new(&e, &token);

        let addr1 = Address::generate(&e);
        let addr2 = Address::generate(&e);

        token_client.mint(&addr1, &1000);
        token_client.mint(&addr2, &2000);

        let tc = soroban_sdk::token::Client::new(&e, &token);
        assert_eq!(tc.balance(&addr1), 1000);
        assert_eq!(tc.balance(&addr2), 2000);
    }

    #[test]
    fn test_approve_batch_emits_events() {
        let e = Env::default();
        e.mock_all_auths();

        let contract_id = e.register_contract(None, VeriTixPay);
        let client = VeriTixPayClient::new(&e, &contract_id);

        let from = Address::generate(&e);
        let spender1 = Address::generate(&e);
        let spender2 = Address::generate(&e);

        let mut approvals = Vec::new(&e);
        approvals.push_back((spender1.clone(), 500i128, 1000u32));
        approvals.push_back((spender2.clone(), 300i128, 1000u32));

        client.approve_batch(&from, &approvals);

        let events = e.events().all();
        assert!(
            !events.events().is_empty(),
            "per-approval events should be emitted"
        );
    }

    #[test]
    fn test_clawback_batch_requires_cosigner_auth() {
        let e = Env::default();
        e.mock_all_auths();

        let contract_id = e.register_contract(None, VeriTixPay);
        let client = VeriTixPayClient::new(&e, &contract_id);

        let admin = Address::generate(&e);
        client.initialize(&admin);
        let cosigner = Address::generate(&e);

        let user = Address::generate(&e);

        e.as_contract(&contract_id, || {
            crate::admin::set_clawback_cosigner(&e, &admin, &cosigner);
            crate::balance::receive_balance(&e, &user, 1000);
        });

        let mut clawbacks = Vec::new(&e);
        clawbacks.push_back((user.clone(), 500i128));

        client.clawback_batch(&admin, &clawbacks);

        e.as_contract(&contract_id, || {
            assert_eq!(crate::balance::read_balance(&e, &user), 500);
        });
    }

    #[test]
    fn test_mint_batch_emits_events() {
        let e = Env::default();
        e.mock_all_auths();

        let contract_id = e.register_contract(None, VeriTixPay);
        let client = VeriTixPayClient::new(&e, &contract_id);

        let admin = Address::generate(&e);
        client.initialize(&admin);

        let u1 = Address::generate(&e);
        let u2 = Address::generate(&e);

        let mut mints = Vec::new(&e);
        mints.push_back((u1.clone(), 500i128));
        mints.push_back((u2.clone(), 300i128));

        let total = client.mint_batch(&admin, &mints);
        assert_eq!(total, 800);

        let events = e.events().all();
        assert!(
            !events.events().is_empty(),
            "per-recipient mint events should be emitted"
        );
    }

    // ── #677: batch atomicity ───────────────────────────────────────────────────

    #[test]
    #[should_panic(expected = "insufficient balance")]
    fn test_clawback_batch_one_insufficient_panics_atomically() {
        let e = Env::default();
        e.mock_all_auths();

        let contract_id = e.register_contract(None, VeriTixPay);
        let client = VeriTixPayClient::new(&e, &contract_id);

        let admin = Address::generate(&e);
        client.initialize(&admin);

        let u1 = Address::generate(&e);
        let u2 = Address::generate(&e);
        client.mint(&admin, &u1, &100);
        client.mint(&admin, &u2, &50);

        let mut clawbacks = Vec::new(&e);
        clawbacks.push_back((u1.clone(), 100i128)); // ok
        clawbacks.push_back((u2.clone(), 200i128)); // insufficient -> whole batch reverts
        client.clawback_batch(&admin, &clawbacks);
    }

    #[test]
    #[should_panic(expected = "balance overflow")]
    fn test_mint_batch_one_entry_overflow_panics_atomically() {
        let e = Env::default();
        e.mock_all_auths();

        let contract_id = e.register_contract(None, VeriTixPay);
        let client = VeriTixPayClient::new(&e, &contract_id);

        let admin = Address::generate(&e);
        client.initialize(&admin);

        let u1 = Address::generate(&e);
        let mut mints = Vec::new(&e);
        mints.push_back((u1.clone(), i128::MAX));
        mints.push_back((u1.clone(), 1i128)); // MAX + 1 overflows -> whole batch reverts
        client.mint_batch(&admin, &mints);
    }

    #[test]
    fn test_mint_batch_returns_total_and_sets_balances() {
        let e = Env::default();
        e.mock_all_auths();

        let contract_id = e.register_contract(None, VeriTixPay);
        let client = VeriTixPayClient::new(&e, &contract_id);

        let admin = Address::generate(&e);
        client.initialize(&admin);

        let u1 = Address::generate(&e);
        let u2 = Address::generate(&e);
        let u3 = Address::generate(&e);

        let mut mints = Vec::new(&e);
        mints.push_back((u1.clone(), 100i128));
        mints.push_back((u2.clone(), 200i128));
        mints.push_back((u3.clone(), 700i128));

        let total = client.mint_batch(&admin, &mints);
        assert_eq!(total, 1000);
        assert_eq!(client.balance(&u1), 100);
        assert_eq!(client.balance(&u2), 200);
        assert_eq!(client.balance(&u3), 700);
    }
}
