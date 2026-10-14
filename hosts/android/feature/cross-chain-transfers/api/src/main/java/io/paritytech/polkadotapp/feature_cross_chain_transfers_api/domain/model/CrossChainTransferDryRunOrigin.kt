package io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.AccountId

sealed class CrossChainTransferDryRunOrigin {
    /**
     * Use fake signed origin that will be topped up to perform the dry run
     * Useful for dry running as the part of fee calculation process
     */
    data object Fake : CrossChainTransferDryRunOrigin()

    /**
     * Use [accountId] as a origin for simulation. Simulation will be done on the current state of the account,
     * without preliminary top ups e.t.c.
     * Useful for final dry run, when all transfer parameters are known and finalized
     */
    class Signed(val accountId: AccountId) : CrossChainTransferDryRunOrigin()
}
