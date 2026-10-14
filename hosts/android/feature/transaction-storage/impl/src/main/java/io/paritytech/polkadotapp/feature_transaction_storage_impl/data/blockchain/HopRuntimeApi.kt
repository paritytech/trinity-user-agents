package io.paritytech.polkadotapp.feature_transaction_storage_impl.data.blockchain

import io.paritytech.polkadotapp.chains.call.RuntimeCallsApi
import io.paritytech.polkadotapp.chains.call.call
import io.paritytech.polkadotapp.chains.util.EncodedArguments.Companion.autoEncodedArgs
import io.paritytech.polkadotapp.common.domain.model.AccountId

private const val SECTION = "HopRuntimeApi"
private const val METHOD = "can_account_promote"

/**
 * Calls `HopRuntimeApi_can_account_promote`. The current runtime implementation returns
 * `account_has_active_authorization(who)` and does not read `data_len`.
 */
suspend fun RuntimeCallsApi.canAccountPromote(who: AccountId, dataLength: UInt = 0u): Boolean {
    return call(
        section = SECTION,
        method = METHOD,
        arguments = autoEncodedArgs(
            "who" to who,
            "data_len" to dataLength,
        ),
    )
}
