package io.paritytech.polkadotapp.feature_prices_impl.data.repository.price.datasource.coingecko

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.util.fullId
import io.paritytech.polkadotapp.common.utils.mapNotNullToSet
import io.paritytech.polkadotapp.feature_prices_api.domain.model.Currency
import io.paritytech.polkadotapp.feature_prices_impl.data.repository.price.datasource.PriceDataSource
import io.paritytech.polkadotapp.feature_prices_impl.data.repository.price.datasource.model.HistoricalPriceRemote
import io.paritytech.polkadotapp.feature_prices_impl.data.repository.price.datasource.model.PriceRemote
import kotlin.time.Duration.Companion.days
import kotlin.time.Duration.Companion.milliseconds

internal class CoingeckoPriceDataSource(
    private val coingeckoApi: CoingeckoApi,
) : PriceDataSource {
    override suspend fun fetchPrices(assets: List<Chain.Asset>, currency: Currency): List<PriceRemote> {
        val priceIds = assets.mapNotNullToSet { it.priceId }

        val pricesByPriceId = coingeckoApi.getPricesByPriceId(priceIds, currency = currency.coinId)

        return assets.mapNotNull { asset ->
            if (asset.priceId == null) return@mapNotNull null

            PriceRemote(
                assetId = asset.fullId,
                price = pricesByPriceId[asset.priceId],
            )
        }
    }

    override suspend fun fetchAllHistoricalPrices(asset: Chain.Asset, currency: Currency): List<HistoricalPriceRemote> {
        val priceId = asset.priceId ?: return emptyList()
        val now = System.currentTimeMillis().milliseconds
        val maxAllowedCoingeckoInterval = 365.days

        val response = coingeckoApi.getCoinRange(
            id = priceId,
            currency = currency.coinId,
            fromTimestamp = (now - maxAllowedCoingeckoInterval).inWholeSeconds,
            toTimestamp = now.inWholeSeconds
        )

        return response.prices.map { (timestampRaw, rateRaw) ->
            HistoricalPriceRemote(
                timestamp = timestampRaw.toLong(),
                assetId = asset.fullId,
                price = rateRaw
            )
        }
    }
}
