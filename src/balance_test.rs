#[cfg(test)]
mod tests {
    use crate::contract::{VeriTixPay, VeriTixPayClient};
    use soroban_sdk::{testutils::Address as _, Address, Env};

    #[test]
    fn test_balance_edge_cases() {
        let e = Env::default();
        e.mock_all_auths();

        let contract_id = e.register_contract(None, VeriTixPay);
        let client = VeriTixPayClient::new(&e, &contract_id);

        let admin = Address::generate(&e);
        client.initialize(&admin);

        let user = Address::generate(&e);
        client.mint(&admin, &user, &1000);
        assert_eq!(client.balance(&user), 1000);

        client.burn(&user, &400);
        assert_eq!(client.balance(&user), 600);
    }

    #[test]
    #[should_panic(expected = "Amount must be strictly positive")]
    fn test_burn_negative_amount_panics() {
        let e = Env::default();
        e.mock_all_auths();

        let contract_id = e.register_contract(None, VeriTixPay);
        let client = VeriTixPayClient::new(&e, &contract_id);

        let admin = Address::generate(&e);
        client.initialize(&admin);

        let user = Address::generate(&e);
        client.mint(&admin, &user, &1000);

        client.burn(&user, &-100);
    }
}
