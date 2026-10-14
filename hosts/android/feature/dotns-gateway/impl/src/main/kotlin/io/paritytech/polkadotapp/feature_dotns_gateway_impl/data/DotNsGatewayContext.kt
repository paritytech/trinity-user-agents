package io.paritytech.polkadotapp.feature_dotns_gateway_impl.data

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext

val BandersnatchContext.Companion.DOTNS_GATEWAY: BandersnatchContext
    get() = BandersnatchContext.fromString("pop:polkadot.network/dotns      ")
