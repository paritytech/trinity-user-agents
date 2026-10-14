package io.paritytech.polkadotapp.feature_usernames_impl.domain.model

import io.paritytech.polkadotapp.tools_jwt_auth_api.domain.error.BackendRequestError

private const val STATUS_AVAILABLE = "AVAILABLE"
private const val STATUS_EXHAUSTED = "EXHAUSTED"
private const val STATUS_INVALID = "INVALID"

sealed interface UsernameAvailabilityState {
    data class Available(val availableDigits: List<String>) : UsernameAvailabilityState
    data object Invalid : UsernameAvailabilityState
    data object Taken : UsernameAvailabilityState

    companion object {
        fun fromStatusAndDigits(status: String?, availableDigits: List<String>): Result<UsernameAvailabilityState> {
            return when (status) {
                STATUS_AVAILABLE -> Result.success(Available(availableDigits))
                STATUS_EXHAUSTED -> Result.success(Taken)
                STATUS_INVALID -> Result.success(Invalid)
                // A missing entry or an unrecognised status is a protocol mismatch, not a
                // verdict on the username — reporting it as Invalid blamed the user.
                else -> Result.failure(BackendRequestError.Malformed)
            }
        }
    }
}
