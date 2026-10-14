package io.paritytech.polkadotapp.feature_prices_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.Timestamp

class HistoricalPrice(
    val timestamp: Timestamp,
    val price: Price,
) {
    companion object {
        fun empty(currency: Currency, timestamp: Timestamp): HistoricalPrice {
            return HistoricalPrice(timestamp, Price.empty(currency))
        }
    }
}
