package io.paritytech.polkadotapp.feature_statement_store_impl.data.blockchain

import io.paritytech.polkadotapp.chains.call.ViewFunctionsApi
import io.paritytech.polkadotapp.chains.call.call
import io.paritytech.polkadotapp.chains.util.EncodedArguments.Companion.noArgs
import io.paritytech.polkadotapp.chains.util.Modules

suspend fun ViewFunctionsApi.getStmtStoreSlotsPerPeriod(): Result<UInt> {
    return call(
        pallet = Modules.RESOURCES,
        name = "get_stmt_store_slots_per_period",
        arguments = noArgs()
    )
}

suspend fun ViewFunctionsApi.getLiteStmtStoreSlotsPerPeriod(): Result<UInt> {
    return call(
        pallet = Modules.RESOURCES,
        name = "get_lite_stmt_store_slots_per_period",
        arguments = noArgs()
    )
}

suspend fun ViewFunctionsApi.getStmtStoreReplacementCooldown(): Result<UInt> {
    return call(
        pallet = Modules.RESOURCES,
        name = "get_stmt_store_replacement_cooldown",
        arguments = noArgs()
    )
}
