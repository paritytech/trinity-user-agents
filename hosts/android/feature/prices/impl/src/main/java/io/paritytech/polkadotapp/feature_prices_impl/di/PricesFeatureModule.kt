package io.paritytech.polkadotapp.feature_prices_impl.di

import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.common.data.network.NetworkApiCreator
import io.paritytech.polkadotapp.common.data.network.create
import io.paritytech.polkadotapp.common.data.storage.SingleValueStorageFactory
import io.paritytech.polkadotapp.feature_prices_api.data.repository.CurrencyRepository
import io.paritytech.polkadotapp.feature_prices_api.data.repository.PriceRepository
import io.paritytech.polkadotapp.feature_prices_api.domain.GetCachedPriceUseCase
import io.paritytech.polkadotapp.feature_prices_api.domain.GetPriceUseCase
import io.paritytech.polkadotapp.feature_prices_api.domain.SyncPricesUseCase
import io.paritytech.polkadotapp.feature_prices_api.presentation.formatter.FiatFormatter
import io.paritytech.polkadotapp.feature_prices_api.presentation.mapper.FiatAmountMapper
import io.paritytech.polkadotapp.feature_prices_impl.data.repository.RealCurrencyRepository
import io.paritytech.polkadotapp.feature_prices_impl.data.repository.price.RealPriceRepository
import io.paritytech.polkadotapp.feature_prices_impl.data.repository.price.datasource.PriceDataSource
import io.paritytech.polkadotapp.feature_prices_impl.data.repository.price.datasource.coingecko.CoingeckoApi
import io.paritytech.polkadotapp.feature_prices_impl.data.repository.price.datasource.coingecko.CoingeckoPriceDataSource
import io.paritytech.polkadotapp.feature_prices_impl.data.storage.CurrencyStorage
import io.paritytech.polkadotapp.feature_prices_impl.data.storage.createCurrencyStorage
import io.paritytech.polkadotapp.feature_prices_impl.domain.RealGetCachedPriceUseCase
import io.paritytech.polkadotapp.feature_prices_impl.domain.RealGetPriceUseCase
import io.paritytech.polkadotapp.feature_prices_impl.domain.RealSyncPricesUseCase
import io.paritytech.polkadotapp.feature_prices_impl.presentation.formatter.RealFiatFormatter
import io.paritytech.polkadotapp.feature_prices_impl.presentation.mapper.RealFiatAmountMapper
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
internal interface PricesFeatureModule {
    companion object {
        @Provides
        @Singleton
        fun provideCoingeckoDataSource(networkApiCreator: NetworkApiCreator): PriceDataSource {
            val api = networkApiCreator.create<CoingeckoApi>(CoingeckoApi.BASE_URL)
            return CoingeckoPriceDataSource(api)
        }

        @Provides
        @Singleton
        fun provideCurrencyStorage(factory: SingleValueStorageFactory): CurrencyStorage {
            return factory.createCurrencyStorage()
        }
    }

    @Binds
    fun bindPriceRepository(real: RealPriceRepository): PriceRepository

    @Binds
    fun bindCurrencyRepository(real: RealCurrencyRepository): CurrencyRepository

    @Binds
    fun bindGetPriceUseCase(real: RealGetPriceUseCase): GetPriceUseCase

    @Binds
    fun bindGetCachedPriceUseCase(real: RealGetCachedPriceUseCase): GetCachedPriceUseCase

    @Binds
    fun bindPriceSyncUseCase(real: RealSyncPricesUseCase): SyncPricesUseCase

    @Binds
    fun bindFiatFormatter(real: RealFiatFormatter): FiatFormatter

    @Binds
    fun bindFiatMapper(real: RealFiatAmountMapper): FiatAmountMapper
}
