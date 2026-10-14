package io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator

/**
 * How [StatementStoreSlotAllocator.allocate] should behave when the target account
 * already has an active statement store slot in the current period.
 */
enum class OnExistingAllocationStrategy {
    /**
     * If the target already has an active slot in the current period skip the on-chain
     * registration and return success immediately.
     */
    IGNORE,

    /**
     * Always submit a fresh `set_statement_store_account` extrinsic, claiming an
     * additional free seq for the target.
     */
    INCREASE,
}
