package io.paritytech.polkadotapp.feature_pgas_api.domain

/**
 * How [PgasClaimer.claim] should behave when the destination account already
 * holds a non-zero PGAS balance.
 */
enum class OnExistingAllocationStrategy {
    /**
     * If the destination already has non-zero PGAS balance skip the on-chain
     * claim and return success immediately.
     */
    IGNORE,

    /**
     * Always submit a fresh `claim_pgas` extrinsic, topping up the destination's
     * PGAS balance.
     */
    INCREASE,
}
