package io.paritytech.polkadotapp.feature_balances_api.data.type.eventDetector

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericEvent
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_balances_api.domain.model.DepositEvent

interface TokenEventDetector {
    fun detectDeposit(event: GenericEvent.Instance): DepositEvent?
}

fun TokenEventDetector.tryDetectDeposit(event: GenericEvent.Instance): DepositEvent? {
    return runCatching { detectDeposit(event) }
        .logFailure("Failed to parse event $event")
        .getOrNull()
}
