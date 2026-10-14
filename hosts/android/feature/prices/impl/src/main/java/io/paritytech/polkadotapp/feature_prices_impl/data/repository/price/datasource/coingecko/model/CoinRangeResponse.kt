package io.paritytech.polkadotapp.feature_prices_impl.data.repository.price.datasource.coingecko.model

import java.math.BigDecimal

internal class CoinRangeResponse(val prices: List<List<BigDecimal>>) {
    class Price(val millis: Long, val price: BigDecimal)
}
