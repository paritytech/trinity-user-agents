package io.paritytech.polkadotapp.feature_coinage_api.domain.model

sealed interface CoinageOnboardingState {
    data object Idle : CoinageOnboardingState
    data object InProgress : CoinageOnboardingState
    data object Completed : CoinageOnboardingState
    data object Failed : CoinageOnboardingState
}
