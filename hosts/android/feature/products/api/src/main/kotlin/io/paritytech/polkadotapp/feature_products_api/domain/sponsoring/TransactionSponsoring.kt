package io.paritytech.polkadotapp.feature_products_api.domain.sponsoring

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId

/**
 * Hook invoked before signing a transaction. An implementation may top up the relevant
 * allowance (e.g. PGAS for Asset Hub Revive calls) so the subsequent on-chain submit
 * does not fail due to exhausted allocation. Returning `Result.success` always allows
 * the sign to proceed; failure aborts the sign.
 */
interface TransactionSponsoring {
    suspend fun sponsorTransaction(
        chainId: ChainId,
        call: GenericCall.Instance,
        account: ProductAccountId,
    ): Result<Unit>
}
