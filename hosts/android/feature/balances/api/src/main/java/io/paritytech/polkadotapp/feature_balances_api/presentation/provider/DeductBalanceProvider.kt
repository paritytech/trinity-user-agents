package io.paritytech.polkadotapp.feature_balances_api.presentation.provider

import io.paritytech.polkadotapp.common.utils.atLeastZero
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import java.math.BigDecimal

class DeductBalanceProvider(
    flow: Flow<BigDecimal>,
    inner: AvailableBalanceProvider,
) : AvailableBalanceProvider {
    override val maxAvailableBalance: Flow<BigDecimal> = combine(
        inner.maxAvailableBalance,
        flow
    ) { innerAvailable, deduct ->
        (innerAvailable - deduct).atLeastZero()
    }.distinctUntilChanged()
}

fun AvailableBalanceProvider.deduct(flow: Flow<BigDecimal>): AvailableBalanceProvider {
    return DeductBalanceProvider(flow, this)
}
