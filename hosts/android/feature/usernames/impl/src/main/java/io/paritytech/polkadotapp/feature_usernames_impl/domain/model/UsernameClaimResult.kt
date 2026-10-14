package io.paritytech.polkadotapp.feature_usernames_impl.domain.model

import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username

sealed interface UsernameClaimResult {
    data class Registered(val username: Username) : UsernameClaimResult

    data class Queued(val username: Username) : UsernameClaimResult

    data object PaymentRequired : UsernameClaimResult
}
