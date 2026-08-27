#[cfg(test)]
mod version_test {
    use crate::contract::VeriTixPayClient;
    use soroban_sdk::{Env, String};

    #[test]
    fn test_version() {
        let e = Env::default();
        e.mock_all_auths();
        let contract_id = e.register_contract(None, crate::contract::VeriTixPay);
        let client = VeriTixPayClient::new(&e, &contract_id);
        let v = client.version();
        assert_eq!(v, String::from_str(&e, "1.0.0"));
    }
}
