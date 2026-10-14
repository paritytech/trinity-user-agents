package io.paritytech.polkadotapp.feature_usernames_impl.domain.model

import io.paritytech.polkadotapp.feature_usernames_impl.domain.error.UsernameFlowError

sealed interface ClaimUsernameOutcome {
    data object Claimed : ClaimUsernameOutcome
    data object Queued : ClaimUsernameOutcome
    data object PaymentRequired : ClaimUsernameOutcome
    data class SuffixTaken(val freshDigits: List<String>) : ClaimUsernameOutcome
    data object Unavailable : ClaimUsernameOutcome
    data class Failed(val error: UsernameFlowError) : ClaimUsernameOutcome
}
