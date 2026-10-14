package io.paritytech.polkadotapp.feature_prices_impl.data.repository.price.datasource.coingecko

import io.paritytech.polkadotapp.common.utils.asQueryParam
import io.paritytech.polkadotapp.feature_prices_impl.data.repository.price.datasource.coingecko.model.CoinRangeResponse
import retrofit2.http.GET
import retrofit2.http.Path
import retrofit2.http.Query
import java.math.BigDecimal

internal interface CoingeckoApi {
    companion object {
        val BASE_URL = "https://api.coingecko.com/api/v3/"
    }

    @GET("simple/price")
    suspend fun getAssetPrice(
        @Query("ids") priceIds: String,
        @Query("vs_currencies") currency: String,
        @Query("include_24hr_change") includeRateChange: Boolean
    ): Map<String, Map<String, Double?>>

    @GET("coins/{id}/market_chart/range")
    suspend fun getCoinRange(
        @Path("id") id: String,
        @Query("vs_currency") currency: String,
        @Query("from") fromTimestamp: Long,
        @Query("to") toTimestamp: Long
    ): CoinRangeResponse
}

internal suspend fun CoingeckoApi.getPricesByPriceId(
    priceIds: Iterable<String>,
    currency: String,
): Map<String, BigDecimal?> {
    return getAssetPrice(
        priceIds = priceIds.asQueryParam(),
        currency = currency,
        includeRateChange = false
    ).mapValues { (_, priceObject) -> priceObject[currency]?.toBigDecimal() }
}
