package io.paritytech.polkadotapp.feature_usernames_impl.domain.model

import io.paritytech.polkadotapp.chains.util.signing.MessageSigningContext

class AccountSigningContexts(
    val peopleLite: MessageSigningContext,
    val dotNsGateway: MessageSigningContext
)
