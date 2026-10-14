package io.paritytech.polkadotapp.feature_pgas_api.domain

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance

/**
 * On-chain configuration of the PGAS claim. Exposed to sponsoring flows that need the
 * claim amount to evaluate whether a top-up is required.
 */
interface PgasClaimSpec {
    suspend fun currentClaimAmount(chainAsset: Chain.Asset): Balance
}
