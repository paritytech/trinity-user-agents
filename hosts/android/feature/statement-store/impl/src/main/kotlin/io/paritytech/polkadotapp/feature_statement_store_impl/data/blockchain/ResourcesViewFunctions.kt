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

/** Highest valid notification seq for a full person; seqs `0..=value` are claimable. */
suspend fun ViewFunctionsApi.getNotificationSlotsPerPeriod(): Result<UByte> {
    return call(
        pallet = Modules.RESOURCES,
        name = "get_notification_slots_per_period",
        arguments = noArgs()
    )
}

/** Highest valid notification seq for a lite person; seqs `0..=value` are claimable. */
suspend fun ViewFunctionsApi.getLiteNotificationSlotsPerPeriod(): Result<UByte> {
    return call(
        pallet = Modules.RESOURCES,
        name = "get_lite_notification_slots_per_period",
        arguments = noArgs()
    )
}
