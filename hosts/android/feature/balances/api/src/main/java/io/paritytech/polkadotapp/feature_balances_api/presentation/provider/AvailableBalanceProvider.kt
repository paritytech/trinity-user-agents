package io.paritytech.polkadotapp.feature_balances_api.presentation.provider

import kotlinx.coroutines.flow.Flow
import java.math.BigDecimal

interface AvailableBalanceProvider {
    val maxAvailableBalance: Flow<BigDecimal>
}
