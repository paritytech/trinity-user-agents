package io.paritytech.polkadotapp.feature_usernames_impl.domain.registrationQueue

sealed interface RegistrationQueueProgress {
    data class Waiting(val position: Int, val initialPosition: Int) : RegistrationQueueProgress

    data object Completed : RegistrationQueueProgress
}
