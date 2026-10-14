package io.paritytech.polkadotapp.feature_settings_impl.presentation.currency.models

import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.vector.ImageVector

@Immutable
data class CurrencyState(
    val selectedCurrency: CurrencyUiModel? = null,
    val availableCurrencies: List<CurrencyUiModel> = emptyList()
)

@Immutable
data class CurrencyUiModel(
    val code: String,
    val name: String,
    val icon: ImageVector
)
