#![cfg(test)]

use super::*;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::{testutils::Address as _, token, Address, Env, String};

// ── RISK SCORE AND EFFECTIVE FEE TESTS ───────────────────────────────────────

fn setup_risk_fee_contract(env: &Env) -> (PaymentContractClient<'_>, Address, Address) {
    let contract_id = env.register(PaymentContract, ());
    let client = PaymentContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin);
    (client, admin, contract_id)
}

#[test]
fn test_low_risk_score_reduces_effective_fee() {
    let env = Env::default();
    let (client, admin, contract_id) = setup_risk_fee_contract(&env);

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let meta = String::from_str(&env, "");

    env.mock_all_auths();

    // Configure base fee and risk-based fee adjustment
    client.set_fee_config(
        &admin,
        &FeeConfig {
            base_fee_bps: 100, // 1%
            min_fee: 1,
            max_fee: 10000,
            fee_recipient: admin.clone(),
        },
    );

    // Set risk scoring thresholds
    client.set_risk_config(
        &admin,
        &RiskConfig {
            low_risk_threshold: 3000,
            high_risk_threshold: 7000,
            low_risk_fee_discount_bps: 50, // 0.5% discount
            high_risk_fee_premium_bps: 150, // 1.5% premium
        },
    );

    // Mint tokens to customer
    token::StellarAssetClient::new(&env, &token).mint(&customer, &10000);
    token::Client::new(&env, &token).approve(&customer, &contract_id, &10000, &10000);

    // Assign low risk score to customer (below 3000)
    client.set_address_risk_score(&admin, &customer, &2500);

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
    
    // Effective fee should be 1% - 0.5% = 0.5% = 5 basis points
    // On 1000 amount: 1000 * 0.005 = 5
    assert_eq!(payment.effective_fee_bps, 50);
    assert!(payment.fee_amount <= 5);
}

#[test]
fn test_high_risk_score_increases_effective_fee() {
    let env = Env::default();
    let (client, admin, contract_id) = setup_risk_fee_contract(&env);

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let meta = String::from_str(&env, "");

    env.mock_all_auths();

    // Configure base fee
    client.set_fee_config(
        &admin,
        &FeeConfig {
            base_fee_bps: 100, // 1%
            min_fee: 1,
            max_fee: 10000,
            fee_recipient: admin.clone(),
        },
    );

    // Set risk scoring thresholds
    client.set_risk_config(
        &admin,
        &RiskConfig {
            low_risk_threshold: 3000,
            high_risk_threshold: 7000,
            low_risk_fee_discount_bps: 50,
            high_risk_fee_premium_bps: 150, // 1.5% premium
        },
    );

    // Mint tokens
    token::StellarAssetClient::new(&env, &token).mint(&customer, &10000);
    token::Client::new(&env, &token).approve(&customer, &contract_id, &10000, &10000);

    // Assign high risk score to customer (above 7000)
    client.set_address_risk_score(&admin, &customer, &8500);

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
    
    // Effective fee should be 1% + 1.5% = 2.5% = 250 basis points
    // On 1000 amount: 1000 * 0.025 = 25
    assert_eq!(payment.effective_fee_bps, 250);
    assert_eq!(payment.fee_amount, 25);
}

#[test]
fn test_neutral_risk_score_maintains_base_fee() {
    let env = Env::default();
    let (client, admin, contract_id) = setup_risk_fee_contract(&env);

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let meta = String::from_str(&env, "");

    env.mock_all_auths();

    // Configure base fee
    client.set_fee_config(
        &admin,
        &FeeConfig {
            base_fee_bps: 100, // 1%
            min_fee: 1,
            max_fee: 10000,
            fee_recipient: admin.clone(),
        },
    );

    // Set risk scoring thresholds
    client.set_risk_config(
        &admin,
        &RiskConfig {
            low_risk_threshold: 3000,
            high_risk_threshold: 7000,
            low_risk_fee_discount_bps: 50,
            high_risk_fee_premium_bps: 150,
        },
    );

    // Mint tokens
    token::StellarAssetClient::new(&env, &token).mint(&customer, &10000);
    token::Client::new(&env, &token).approve(&customer, &contract_id, &10000, &10000);

    // Assign neutral risk score to customer (between 3000 and 7000)
    client.set_address_risk_score(&admin, &customer, &5000);

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
    
    // Effective fee should remain at base 1% = 100 basis points
    // On 1000 amount: 1000 * 0.01 = 10
    assert_eq!(payment.effective_fee_bps, 100);
    assert_eq!(payment.fee_amount, 10);
}
