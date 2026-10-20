#![cfg(test)]

use super::*;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::{testutils::Address as _, token, Address, Env, String};

// ── CONTRACT AND FUNCTION PAUSE CONTROL TESTS ─────────────────────────────────

fn setup_pause_contract(env: &Env) -> (PaymentContractClient<'_>, Address, Address) {
    let contract_id = env.register(PaymentContract, ());
    let client = PaymentContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin);
    (client, admin, contract_id)
}

#[test]
fn test_global_contract_pause_blocks_all_operations() {
    let env = Env::default();
    let (client, admin, contract_id) = setup_pause_contract(&env);

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let meta = String::from_str(&env, "");

    env.mock_all_auths();

    // Mint tokens and approve
    token::StellarAssetClient::new(&env, &token).mint(&customer, &5000);
    token::Client::new(&env, &token).approve(&customer, &contract_id, &5000, &10000);

    // Admin pauses the entire contract
    client.pause_contract(&admin, &true);

    // Verify contract is paused
    assert!(client.is_contract_paused());

    // Attempt to create payment - should fail
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.create_payment(
            &customer,
            &merchant,
            &1000,
            &token,
            &Currency::USDC,
            &0,
            &meta,
        );
    }));

    assert!(result.is_err());

    // Admin unpauses the contract
    client.pause_contract(&admin, &false);
    assert!(!client.is_contract_paused());

    // Now payment creation should succeed
    env.ledger().set_timestamp(1000);
    let payment_id = client.create_payment(
        &customer,
        &merchant,
        &1000,
        &token,
        &Currency::USDC,
        &0,
        &meta,
    );

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.amount, 1000);
    assert_eq!(payment.status, PaymentStatus::Pending);
}

#[test]
fn test_function_specific_pause_blocks_only_target_function() {
    let env = Env::default();
    let (client, admin, contract_id) = setup_pause_contract(&env);

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let meta = String::from_str(&env, "");

    env.mock_all_auths();

    // Mint tokens
    token::StellarAssetClient::new(&env, &token).mint(&customer, &10000);
    token::Client::new(&env, &token).approve(&customer, &contract_id, &10000, &10000);

    // Create a payment first
    env.ledger().set_timestamp(1000);
    let payment_id = client.create_payment(
        &customer,
        &merchant,
        &1000,
        &token,
        &Currency::USDC,
        &0,
        &meta,
    );

    // Pause only the refund_payment function
    client.pause_function(&admin, &String::from_str(&env, "refund_payment"), &true);

    // Verify function is paused
    assert!(client.is_function_paused(&String::from_str(&env, "refund_payment")));

    // Attempt to refund - should fail
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.refund_payment(&customer, &payment_id);
    }));

    assert!(result.is_err());

    // But creating another payment should still work
    let payment_id_2 = client.create_payment(
        &customer,
        &merchant,
        &500,
        &token,
        &Currency::USDC,
        &0,
        &meta,
    );

    let payment2 = client.get_payment(&payment_id_2);
    assert_eq!(payment2.amount, 500);

    // Unpause the refund function
    client.pause_function(&admin, &String::from_str(&env, "refund_payment"), &false);
    assert!(!client.is_function_paused(&String::from_str(&env, "refund_payment")));

    // Now refund should work
    client.refund_payment(&customer, &payment_id);
    let refunded_payment = client.get_payment(&payment_id);
    assert_eq!(refunded_payment.status, PaymentStatus::Refunded);
}

#[test]
fn test_multiple_functions_paused_independently() {
    let env = Env::default();
    let (client, admin, contract_id) = setup_pause_contract(&env);

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let meta = String::from_str(&env, "");

    env.mock_all_auths();

    // Mint tokens
    token::StellarAssetClient::new(&env, &token).mint(&customer, &10000);
    token::Client::new(&env, &token).approve(&customer, &contract_id, &10000, &10000);

    // Pause multiple functions
    client.pause_function(&admin, &String::from_str(&env, "create_payment"), &true);
    client.pause_function(&admin, &String::from_str(&env, "schedule_payment"), &true);

    // Verify both functions are paused
    assert!(client.is_function_paused(&String::from_str(&env, "create_payment")));
    assert!(client.is_function_paused(&String::from_str(&env, "schedule_payment")));

    // Attempt to create immediate payment - should fail
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.create_payment(
            &customer,
            &merchant,
            &1000,
            &token,
            &Currency::USDC,
            &0,
            &meta,
        );
    }));
    assert!(result.is_err());

    // Attempt to schedule payment - should also fail
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        env.ledger().set_timestamp(1000);
        client.schedule_payment(&customer, &merchant, &token, &1000, &2000);
    }));
    assert!(result.is_err());

    // Unpause only create_payment
    client.pause_function(&admin, &String::from_str(&env, "create_payment"), &false);
    assert!(!client.is_function_paused(&String::from_str(&env, "create_payment")));
    assert!(client.is_function_paused(&String::from_str(&env, "schedule_payment")));

    // Now create_payment should work
    env.ledger().set_timestamp(1000);
    let payment_id = client.create_payment(
        &customer,
        &merchant,
        &1000,
        &token,
        &Currency::USDC,
        &0,
        &meta,
    );

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.amount, 1000);

    // But schedule_payment should still fail
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.schedule_payment(&customer, &merchant, &token, &1000, &2000);
    }));
    assert!(result.is_err());
}
