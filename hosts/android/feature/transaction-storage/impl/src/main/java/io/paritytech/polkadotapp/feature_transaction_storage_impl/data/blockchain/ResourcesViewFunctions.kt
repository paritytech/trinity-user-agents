package io.paritytech.polkadotapp.feature_transaction_storage_impl.data.blockchain

import io.paritytech.polkadotapp.chains.call.ViewFunctionsApi
import io.paritytech.polkadotapp.chains.call.call
import io.paritytech.polkadotapp.chains.util.EncodedArguments.Companion.noArgs
import io.paritytech.polkadotapp.chains.util.Modules

suspend fun ViewFunctionsApi.getLongTermStorageClaimsPerPeriod(): Result<UByte> {
    return call(
        pallet = Modules.RESOURCES,
        name = "get_long_term_storage_claims_per_period",
        arguments = noArgs()
    )
}
