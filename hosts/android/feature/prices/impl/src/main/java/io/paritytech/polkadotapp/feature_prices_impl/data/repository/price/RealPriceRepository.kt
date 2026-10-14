package io.paritytech.polkadotapp.feature_prices_impl.data.repository.price

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.chains.util.databaseId
import io.paritytech.polkadotapp.chains.util.toDomain
import io.paritytech.polkadotapp.chains.util.toLocal
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.inBackground
import io.paritytech.polkadotapp.common.utils.network.NetworkStateService
import io.paritytech.polkadotapp.common.utils.network.withNetworkRetries
import io.paritytech.polkadotapp.database.dao.TokenPriceDao
import io.paritytech.polkadotapp.database.model.TokenPriceLocal
import io.paritytech.polkadotapp.feature_prices_api.data.repository.PriceRepository
import io.paritytech.polkadotapp.feature_prices_api.domain.model.Currency
import io.paritytech.polkadotapp.feature_prices_api.domain.model.HistoricalPrice
import io.paritytech.polkadotapp.feature_prices_api.domain.model.Price
import io.paritytech.polkadotapp.feature_prices_api.domain.model.PriceLookup
import io.paritytech.polkadotapp.feature_prices_impl.data.repository.price.datasource.PriceDataSource
import io.paritytech.polkadotapp.feature_prices_impl.data.repository.price.datasource.model.PriceRemote
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChangedBy
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.withContext
import javax.inject.Inject

internal class RealPriceRepository @Inject constructor(
    private val priceDataSource: PriceDataSource,
    private val coroutineDispatchers: CoroutineDispatchers,
    private val priceDao: TokenPriceDao,
    private val networkStateService: NetworkStateService,
) : PriceRepository {
    override suspend fun syncPrices(currency: Currency, allAssets: List<Chain.Asset>): Result<Unit> {
        return withContext(coroutineDispatchers.io) {
            networkStateService.withNetworkRetries {
                val prices = priceDataSource.fetchPrices(allAssets, currency)
                priceDao.insert(prices.toLocal(currency))
            }
        }
    }

    override suspend fun getPrice(chainAsset: Chain.Asset, currency: Currency): Price {
        return withContext(coroutineDispatchers.io) {
            priceDao.getPrice(chainAsset.chainId, chainAsset.id, currency.id)
                .toPrice(currency)
        }
    }

    override fun priceFlow(chainAsset: Chain.Asset, currency: Currency): Flow<Price> {
        return priceDao.priceFlow(chainAsset.chainId, chainAsset.id, currency.id)
            .map { it.toPrice(currency) }
            .distinctUntilChangedBy { it.perUnitPrice }
            .inBackground()
    }

    override suspend fun getAllPrices(currency: Currency): PriceLookup {
        return withContext(coroutineDispatchers.io) {
            priceDao.allPrices(currency.id).toPriceLookup(currency)
        }
    }

    override suspend fun getPrices(
        currency: Currency,
        assets: List<Chain.Asset>
    ): PriceLookup {
        return withContext(coroutineDispatchers.io) {
            val ids = assets.map { it.databaseId }
            priceDao.getPrices(currency.id, ids).toPriceLookup(currency)
        }
    }

    override fun allPricesFlow(currency: Currency): Flow<PriceLookup> {
        return priceDao.allPricesFlow(currency.id)
            .map { it.toPriceLookup(currency) }
            .inBackground()
    }

    override suspend fun fetchAllHistoricalRates(
        chainAsset: Chain.Asset,
        currency: Currency
    ): Result<List<HistoricalPrice>> {
        return withContext(coroutineDispatchers.io) {
            networkStateService.withNetworkRetries {
                priceDataSource.fetchAllHistoricalPrices(chainAsset, currency).map {
                    HistoricalPrice(
                        timestamp = it.timestamp,
                        price = Price(it.price, currency),
                    )
                }
            }
        }
    }

    private fun List<PriceRemote>.toLocal(currency: Currency): List<TokenPriceLocal> {
        return map {
            TokenPriceLocal(
                assetId = it.assetId.toLocal(),
                currencyId = currency.id,
                price = it.price
            )
        }
    }

    @JvmName("toPriceOrEmpty")
    private fun TokenPriceLocal?.toPrice(currency: Currency): Price {
        return this?.toPrice(currency) ?: Price.empty(currency)
    }

    private fun TokenPriceLocal.toPrice(currency: Currency): Price {
        return Price(
            perUnitPrice = price,
            currency = currency
        )
    }

    private fun List<TokenPriceLocal>.toPriceLookup(currency: Currency): PriceLookup {
        val allPrices = associateBy(
            keySelector = { it.assetId.toDomain() },
            valueTransform = { it.toPrice(currency) }
        )

        return RealPriceLookup(allPrices, currency)
    }

    private class RealPriceLookup(
        private val allPrices: Map<FullChainAssetId, Price>,
        private val currency: Currency
    ) : PriceLookup {
        override fun get(fullChainAssetId: FullChainAssetId): Price {
            return allPrices[fullChainAssetId] ?: Price.empty(currency)
        }
    }
}
