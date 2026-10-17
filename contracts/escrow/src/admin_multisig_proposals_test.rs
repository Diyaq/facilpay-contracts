#![cfg(test)]

use super::*;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::{testutils::Address as _, vec, Address, Env, String, Vec};

// ── ADMIN MULTISIG PROPOSALS TESTS ────────────────────────────────────────────

fn setup_multisig_contract(env: &Env) -> (EscrowContractClient<'_>, Vec<Address>) {
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(env, &contract_id);

    // Create multiple admins for multisig
    let admin1 = Address::generate(env);
    let admin2 = Address::generate(env);
    let admin3 = Address::generate(env);
    
    let admins = vec![env, admin1.clone(), admin2.clone(), admin3.clone()];
    
    env.mock_all_auths();
    
    (client, admins)
}

#[test]
fn test_multisig_proposal_creation_and_approval() {
    let env = Env::default();
    let (client, admins) = setup_multisig_contract(&env);

    // Configure multisig with 2-of-3 threshold
    client.set_multisig_config(
        &admins.get(0).unwrap(),
        &MultisigConfig {
            admins: admins.clone(),
            threshold: 2,
            proposal_expiry_duration: 86400, // 24 hours
        },
    );

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = Address::generate(&env);

    env.ledger().set_timestamp(1000);
    let escrow_id = client.create_escrow(&customer, &merchant, &5000_i128, &token, &3600_u64, &0_u64);

    // Admin1 creates a proposal to release escrow
    let proposal_id = client.create_multisig_proposal(
        &admins.get(0).unwrap(),
        &ProposalAction::ReleaseEscrow,
        &escrow_id,
        &String::from_str(&env, "Release approved work"),
    );

    // Admin2 approves the proposal
    client.approve_multisig_proposal(&admins.get(1).unwrap(), &proposal_id);

    let proposal = client.get_multisig_proposal(&proposal_id);
    assert_eq!(proposal.approval_count, 2);
    assert_eq!(proposal.status, ProposalStatus::Approved);
    assert!(proposal.executed);
    
    // Verify escrow was released
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
}

#[test]
fn test_multisig_proposal_rejection_with_threshold() {
    let env = Env::default();
    let (client, admins) = setup_multisig_contract(&env);

    // Configure multisig with 3-of-3 threshold (requires unanimous approval)
    client.set_multisig_config(
        &admins.get(0).unwrap(),
        &MultisigConfig {
            admins: admins.clone(),
            threshold: 3,
            proposal_expiry_duration: 86400,
        },
    );

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = Address::generate(&env);

    env.ledger().set_timestamp(1000);
    let escrow_id = client.create_escrow(&customer, &merchant, &5000_i128, &token, &3600_u64, &0_u64);

    // Admin1 creates a proposal to refund escrow
    let proposal_id = client.create_multisig_proposal(
        &admins.get(0).unwrap(),
        &ProposalAction::RefundEscrow,
        &escrow_id,
        &String::from_str(&env, "Customer requested refund"),
    );

    // Admin2 approves
    client.approve_multisig_proposal(&admins.get(1).unwrap(), &proposal_id);

    // Admin3 rejects
    client.reject_multisig_proposal(&admins.get(2).unwrap(), &proposal_id);

    let proposal = client.get_multisig_proposal(&proposal_id);
    assert_eq!(proposal.approval_count, 2);
    assert_eq!(proposal.rejection_count, 1);
    assert_eq!(proposal.status, ProposalStatus::Rejected);
    assert!(!proposal.executed);
    
    // Verify escrow was NOT refunded
    let escrow = client.get_escrow(&escrow_id);
    assert_ne!(escrow.status, EscrowStatus::Refunded);
}

#[test]
fn test_multisig_proposal_expiry_prevents_execution() {
    let env = Env::default();
    let (client, admins) = setup_multisig_contract(&env);

    // Configure multisig with 2-of-3 threshold and short expiry
    client.set_multisig_config(
        &admins.get(0).unwrap(),
        &MultisigConfig {
            admins: admins.clone(),
            threshold: 2,
            proposal_expiry_duration: 3600, // 1 hour
        },
    );

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token = Address::generate(&env);

    env.ledger().set_timestamp(1000);
    let escrow_id = client.create_escrow(&customer, &merchant, &5000_i128, &token, &7200_u64, &0_u64);

    // Admin1 creates a proposal
    let proposal_id = client.create_multisig_proposal(
        &admins.get(0).unwrap(),
        &ProposalAction::ReleaseEscrow,
        &escrow_id,
        &String::from_str(&env, "Release with delay"),
    );

    // Time passes beyond expiry
    env.ledger().set_timestamp(1000 + 3601);

    // Admin2 tries to approve after expiry - should fail or mark as expired
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.approve_multisig_proposal(&admins.get(1).unwrap(), &proposal_id);
    }));

    assert!(result.is_err());

    let proposal = client.get_multisig_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::Expired);
    assert!(!proposal.executed);
}
