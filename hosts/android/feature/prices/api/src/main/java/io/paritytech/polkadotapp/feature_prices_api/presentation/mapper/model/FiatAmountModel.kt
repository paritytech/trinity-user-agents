package io.paritytech.polkadotapp.feature_prices_api.presentation.mapper.model

import androidx.compose.runtime.Immutable
import java.math.BigDecimal

@Immutable
data class FiatAmountModel(
    val fiatAmount: BigDecimal,
    val currencyDisplay: String
)
