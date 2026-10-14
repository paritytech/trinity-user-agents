package io.paritytech.polkadotapp.feature_onboarding_impl.presentation.mnemonic.confirm.models

import androidx.compose.runtime.Immutable

@Immutable
data class WordUiModel(
    val selectedIndex: Int,
    val value: String
)
