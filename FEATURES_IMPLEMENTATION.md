# New Escrow Features Implementation

This document describes the four new features added to the escrow contract.

## Feature 1: Per-Escrow Third-Party Dispute Resolver

### Description
Allows customer or merchant to assign a specific third-party address as the dispute resolver for an individual escrow, rather than using the default contract admins.

### Storage Keys Added
- `ThirdPartyResolver(u64)` - Stores resolver info per escrow

### Data Structures
```rust
pub struct ThirdPartyResolver {
    pub escrow_id: u64,
    pub resolver: Address,
    pub assigned_by: Address,
    pub assigned_at: u64,
}
```

### Events
- `ThirdPartyResolverAssigned` - Emitted when resolver is assigned
- `ThirdPartyResolverRemoved` - Emitted when resolver is removed

### Functions Added
1. `assign_third_party_resolver(env, caller, escrow_id, resolver)` - Assigns a resolver
2. `remove_third_party_resolver(env, caller, escrow_id)` - Removes the resolver
3. `get_third_party_resolver(env, escrow_id)` - Returns resolver info

### Integration
- Modified `resolve_dispute()` to check for third-party resolver first
- Modified `resolve_dispute_split()` to check for third-party resolver first
- Resolver must be different from customer and merchant

### Errors Added
- `ResolverMustBeDifferent = 328` - Resolver cannot be customer or merchant

---

## Feature 2: Merchant Acceptance Step Before Funds Lock

### Description
Adds an optional merchant acceptance step where the merchant must explicitly accept an escrow before certain operations can proceed.

### Storage Keys Added
- `MerchantAcceptance(u64)` - Stores acceptance status per escrow

### Data Structures
```rust
pub struct MerchantAcceptance {
    pub escrow_id: u64,
    pub accepted: bool,
    pub accepted_at: Option<u64>,
}
```

### Events
- `MerchantAccepted` - Emitted when merchant accepts escrow

### Functions Added
1. `merchant_accept_escrow(env, merchant, escrow_id)` - Merchant accepts escrow
2. `get_merchant_acceptance(env, escrow_id)` - Returns acceptance status
3. `require_merchant_acceptance(env, escrow_id)` - Internal validation (private)

### Integration
- Added internal helper `require_merchant_acceptance()` for future integration
- Can be called before critical operations like release
- Backwards compatible (no acceptance record = operation allowed)

### Errors Added
- `MerchantAcceptanceRequired = 323` - Acceptance required but not given
- `MerchantAlreadyAccepted = 324` - Already accepted

---

## Feature 3: Terms Hash with Two-Party Amendment

### Description
Allows storing a hash of escrow terms and enabling both parties to propose and approve amendments through a two-party approval process.

### Storage Keys Added
- `TermsHash(u64)` - Current terms hash per escrow
- `TermsAmendment(u64, u64)` - Amendment proposals (escrow_id, amendment_index)
- `TermsAmendmentCount(u64)` - Number of amendments per escrow

### Data Structures
```rust
pub struct TermsHash {
    pub escrow_id: u64,
    pub hash: BytesN<32>,
    pub created_at: u64,
}

pub struct TermsAmendment {
    pub escrow_id: u64,
    pub amendment_index: u64,
    pub new_hash: BytesN<32>,
    pub customer_approved: bool,
    pub merchant_approved: bool,
    pub customer_approved_at: Option<u64>,
    pub merchant_approved_at: Option<u64>,
    pub finalized: bool,
    pub finalized_at: Option<u64>,
}
```

### Events
- `TermsHashSet` - Emitted when initial terms hash is set
- `TermsAmendmentProposed` - Emitted when amendment is proposed
- `TermsAmendmentApproved` - Emitted when party approves amendment
- `TermsAmendmentFinalized` - Emitted when both parties approve and amendment is applied

### Functions Added
1. `set_terms_hash(env, caller, escrow_id, hash)` - Sets initial terms hash
2. `propose_terms_amendment(env, proposer, escrow_id, new_hash)` - Proposes amendment
3. `approve_terms_amendment(env, approver, escrow_id, amendment_index)` - Approves amendment
4. `get_terms_hash(env, escrow_id)` - Returns current terms hash
5. `get_terms_amendment(env, escrow_id, amendment_index)` - Returns specific amendment
6. `get_all_terms_amendments(env, escrow_id)` - Returns all amendments for an escrow

### Workflow
1. Either party sets initial terms hash
2. Either party proposes amendment (auto-approves for proposer)
3. Other party approves amendment
4. When both approve, amendment is finalized and becomes new terms hash

### Errors Added
- `InvalidTermsHash = 325` - Invalid or not found
- `AmendmentRequiresBothParties = 326` - Both parties must approve (not used yet, reserved)

---

## Feature 4: Pause Escrow Creation for a Single Merchant

### Description
Allows admins to pause/unpause escrow creation for specific merchants individually, separate from global contract pause.

### Storage Keys Added
- `MerchantPauseState(Address)` - Pause state per merchant

### Data Structures
```rust
pub struct MerchantPauseState {
    pub merchant: Address,
    pub paused: bool,
    pub paused_at: Option<u64>,
    pub paused_by: Address,
    pub reason: String,
}
```

### Events
- `MerchantEscrowCreationPaused` - Emitted when merchant is paused
- `MerchantEscrowCreationUnpaused` - Emitted when merchant is unpaused

### Functions Added
1. `pause_merchant_escrow_creation(env, admin, merchant, reason)` - Pauses merchant
2. `unpause_merchant_escrow_creation(env, admin, merchant)` - Unpauses merchant
3. `get_merchant_pause_state(env, merchant)` - Returns pause state

### Integration
- Modified `internal_create_escrow()` to check merchant pause state at the beginning
- Check happens before migration check and other validations

### Errors Added
- `MerchantCreationPaused = 327` - Merchant cannot create escrows

---

## Error Code Summary

New error codes added (all in `ActionError` enum):
- 323: `MerchantAcceptanceRequired`
- 324: `MerchantAlreadyAccepted`
- 325: `InvalidTermsHash`
- 326: `AmendmentRequiresBothParties`
- 327: `MerchantCreationPaused`
- 328: `ResolverMustBeDifferent`

Error range updated: `(300..=328)` in error conversion

---

## Files Modified

1. `/home/olowo/Desktop/drips/facilpay-contracts/contracts/escrow/src/lib.rs`
   - Added new storage keys to `EscrowKey` enum
   - Added new error variants to `ActionError` enum
   - Added new data structures and events
   - Added new public functions for all features
   - Modified `internal_create_escrow()` for merchant pause check
   - Modified `resolve_dispute()` for third-party resolver support
   - Modified `resolve_dispute_split()` for third-party resolver support
   - Updated error code range in `try_from` implementation

---

## Usage Examples

### Third-Party Resolver
```rust
// Assign resolver
escrow_contract.assign_third_party_resolver(&env, &customer, escrow_id, &resolver);

// Resolver can now resolve disputes
escrow_contract.resolve_dispute(&env, &resolver, escrow_id, true);

// Remove resolver
escrow_contract.remove_third_party_resolver(&env, &customer, escrow_id);
```

### Merchant Acceptance
```rust
// Merchant accepts escrow
escrow_contract.merchant_accept_escrow(&env, &merchant, escrow_id);

// Check acceptance status
let acceptance = escrow_contract.get_merchant_acceptance(&env, escrow_id);
```

### Terms Hash Amendment
```rust
// Set initial terms
escrow_contract.set_terms_hash(&env, &customer, escrow_id, &terms_hash);

// Propose amendment
let amendment_idx = escrow_contract.propose_terms_amendment(&env, &merchant, escrow_id, &new_hash);

// Other party approves
escrow_contract.approve_terms_amendment(&env, &customer, escrow_id, amendment_idx);
// Now finalized and new_hash becomes current terms
```

### Merchant Pause
```rust
// Pause merchant
escrow_contract.pause_merchant_escrow_creation(&env, &admin, &merchant, &String::from_str(&env, "fraud investigation"));

// Try to create escrow (will fail with MerchantCreationPaused)
// escrow_contract.create_escrow(...); // Error!

// Unpause merchant
escrow_contract.unpause_merchant_escrow_creation(&env, &admin, &merchant);
```

---

## Notes

1. All features are backwards compatible
2. Third-party resolver takes precedence over contract admins for dispute resolution
3. Merchant acceptance is optional (no record = allowed for backwards compatibility)
4. Terms amendments require both parties' explicit approval
5. Merchant pause is independent of global contract pause
6. All functions respect existing pause mechanisms via `require_not_paused()`
7. All state-changing functions call `bump_instance_ttl()` for storage extension
8. All functions emit appropriate events for off-chain tracking
