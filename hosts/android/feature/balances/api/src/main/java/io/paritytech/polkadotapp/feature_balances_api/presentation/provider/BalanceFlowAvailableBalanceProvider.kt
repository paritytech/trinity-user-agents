package io.paritytech.polkadotapp.feature_balances_api.presentation.provider

import kotlinx.coroutines.flow.Flow
import java.math.BigDecimal

class BalanceFlowAvailableBalanceProvider(balance: Flow<BigDecimal>) : AvailableBalanceProvider {
    override val maxAvailableBalance = balance
}
