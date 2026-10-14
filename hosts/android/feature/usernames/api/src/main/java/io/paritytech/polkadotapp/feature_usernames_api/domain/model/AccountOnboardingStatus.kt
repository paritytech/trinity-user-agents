package io.paritytech.polkadotapp.feature_usernames_api.domain.model

data class AccountOnboardingStatus(
    val accountCreated: Boolean,
    val usernameClaimed: Username?,
    val queuedUsername: Username?
) {
    val isOnboarded: Boolean = accountCreated && usernameClaimed != null

    val isWaitingInQueue: Boolean = accountCreated && usernameClaimed == null && queuedUsername != null

    companion object {
        val EMPTY = AccountOnboardingStatus(false, null, null)
    }
}
