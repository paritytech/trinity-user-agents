package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.utils.padEnd

fun omniPoolAccountId(): AccountId {
    return "modlomnipool".encodeToByteArray().padEnd(expectedSize = 32, padding = 0)
        .intoAccountId()
}
