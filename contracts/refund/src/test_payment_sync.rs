#![cfg(test)]

//! Tests for #809: processed refunds are mirrored onto the payment contract.

use super::*;
use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{contract, contracterror, contractimpl, token, Address, Env, String};

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MockPaymentError {
    Rejected = 1,
}

#[contracttype]
enum MockPaymentKey {
    Reject,
    Calls,
}

/// Stand-in for the payment contract's `check_payment_customer` / `record_refund`.
#[contract]
pub struct MockPayment;

#[contractimpl]
impl MockPayment {
    pub fn check_payment_customer(_env: Env, _payment_id: u64, _customer: Address) -> bool {
        true
    }

    pub fn set_reject(env: Env, reject: bool) {
        env.storage()
            .instance()
            .set(&MockPaymentKey::Reject, &reject);
    }

    pub fn record_refund(
        env: Env,
        refund_contract: Address,
        payment_id: u64,
        refund_id: u64,
        amount: i128,
    ) -> Result<(), MockPaymentError> {
        refund_contract.require_auth();
        if env
            .storage()
            .instance()
            .get(&MockPaymentKey::Reject)
            .unwrap_or(false)
        {
            return Err(MockPaymentError::Rejected);
        }
        let mut calls = Self::calls(env.clone());
        calls.push_back((refund_contract, payment_id, refund_id, amount));
        env.storage().instance().set(&MockPaymentKey::Calls, &calls);
        Ok(())
    }

    pub fn calls(env: Env) -> Vec<(Address, u64, u64, i128)> {
        env.storage()
            .instance()
            .get(&MockPaymentKey::Calls)
            .unwrap_or(Vec::new(&env))
    }
}

struct Setup<'a> {
    env: Env,
    client: RefundContractClient<'a>,
    admin: Address,
    merchant: Address,
    customer: Address,
    token: Address,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    token::StellarAssetClient::new(&env, &token).mint(&contract_id, &1_000_000);

    Setup {
        merchant: Address::generate(&env),
        customer: Address::generate(&env),
        env,
        client,
        admin,
        token,
    }
}

fn register_mock_payment<'a>(s: &Setup) -> MockPaymentClient<'a> {
    let id = s.env.register(MockPayment, ());
    s.client.set_payment_contract_address(&s.admin, &id);
    MockPaymentClient::new(&s.env, &id)
}

fn approved_refund(s: &Setup, payment_id: u64, amount: i128) -> u64 {
    let refund_id = s.client.request_refund(
        &s.merchant,
        &payment_id,
        &s.customer,
        &amount,
        &1_000,
        &s.token,
        &String::from_str(&s.env, "damaged"),
        &RefundReasonCode::CustomerRequest,
        &s.env.ledger().timestamp(),
    );
    s.client.approve_refund(&s.admin, &refund_id);
    refund_id
}

fn has_event(env: &Env, name: &str) -> bool {
    let expected = Symbol::new(env, name);
    env.events().all().iter().any(|(_, topics, _)| {
        topics
            .get(0)
            .and_then(|t| Symbol::try_from_val(env, &t).ok())
            .is_some_and(|s| s == expected)
    })
}

#[test]
fn processed_refund_is_recorded_on_payment_contract() {
    let s = setup();
    let payment = register_mock_payment(&s);
    let refund_id = approved_refund(&s, 7, 400);

    s.client.process_refund(&s.admin, &refund_id);

    assert!(has_event(&s.env, "refund_payment_synced"));
    assert!(s.client.is_refund_synced(&refund_id));
    assert_eq!(
        payment.calls(),
        Vec::from_array(
            &s.env,
            [(s.client.address.clone(), 7u64, refund_id, 400i128)]
        )
    );
    assert_eq!(
        s.client.try_retry_refund_sync(&refund_id),
        Err(Ok(Error::Ext(ExtError::RefundAlreadySynced)))
    );
}

#[test]
fn sync_is_skipped_safely_without_payment_contract() {
    let s = setup();
    let refund_id = approved_refund(&s, 7, 400);

    // Processing still pays out and succeeds.
    s.client.process_refund(&s.admin, &refund_id);

    assert!(has_event(&s.env, "refund_payment_sync_failed"));
    assert_eq!(
        s.client.get_refund(&refund_id).status,
        RefundStatus::Processed
    );
    assert_eq!(
        token::Client::new(&s.env, &s.token).balance(&s.customer),
        400
    );
    assert!(!s.client.is_refund_synced(&refund_id));
    assert_eq!(
        s.client.try_retry_refund_sync(&refund_id),
        Err(Ok(Error::Core(CoreError::PaymentContractNotSet)))
    );
}

#[test]
fn rejected_sync_keeps_payout_and_can_be_retried() {
    let s = setup();
    let payment = register_mock_payment(&s);
    payment.set_reject(&true);
    let refund_id = approved_refund(&s, 7, 400);

    s.client.process_refund(&s.admin, &refund_id);

    assert!(has_event(&s.env, "refund_payment_sync_failed"));
    assert_eq!(
        s.client.get_refund(&refund_id).status,
        RefundStatus::Processed
    );
    assert_eq!(
        token::Client::new(&s.env, &s.token).balance(&s.customer),
        400
    );
    assert!(!s.client.is_refund_synced(&refund_id));
    assert!(!s.client.retry_refund_sync(&refund_id));
    assert_eq!(payment.calls().len(), 0);

    payment.set_reject(&false);
    assert!(s.client.retry_refund_sync(&refund_id));
    assert!(s.client.is_refund_synced(&refund_id));
    assert_eq!(payment.calls().len(), 1);
}

#[test]
fn retry_rejects_unknown_and_unprocessed_refunds() {
    let s = setup();
    register_mock_payment(&s);
    let refund_id = approved_refund(&s, 7, 400);

    assert_eq!(
        s.client.try_retry_refund_sync(&refund_id),
        Err(Ok(Error::Core(CoreError::InvalidStatus)))
    );
    assert_eq!(
        s.client.try_retry_refund_sync(&999),
        Err(Ok(Error::Core(CoreError::RefundNotFound)))
    );
}
