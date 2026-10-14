package io.paritytech.polkadotapp.feature_usernames_impl.domain.model

sealed interface RegistrationQueueStatus {
    data class InQueue(
        val position: Int,
        val group: Int,
        val estimatedIterationsRemaining: Int
    ) : RegistrationQueueStatus

    data object NotQueued : RegistrationQueueStatus
}
