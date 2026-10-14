package io.paritytech.polkadotapp.feature_transaction_storage_api.domain.slotAllocator

/**
 * How [TransactionStorageSlotAllocator.allocate] should behave when the target account
 * already has an authorization on the Bullet-In chain.
 */
enum class OnExistingAllocationStrategy {
    /**
     * If the target already has an authorization (allocation > 0) skip the on-chain
     * claim and return success immediately.
     */
    IGNORE,

    /**
     * Always submit a fresh `claim_long_term_storage` extrinsic, growing the existing
     * allocation if there is one.
     */
    INCREASE,
}
