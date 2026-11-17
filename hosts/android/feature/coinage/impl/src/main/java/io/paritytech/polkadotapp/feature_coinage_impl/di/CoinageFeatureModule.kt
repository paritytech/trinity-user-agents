package io.paritytech.polkadotapp.feature_coinage_impl.di

import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.common.data.storage.SingleValueStorageFactory
import io.paritytech.polkadotapp.feature_coinage_api.domain.debug.CoinageDebugSettings
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentPlanner
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentService
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinageRecyclingStrategySettings
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageAccountBackupObserver
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageBackupService
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageServiceStarter
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.ClaimReceivedCoinsUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageHoldingsUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageOperationStatusUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentStatusUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageTestnetFundUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.OnboardingUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.PrepareCoinageTransferUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.ShareCoinageLogsUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TotalBalanceUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.core.CoreClaimReceivedCoinsUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.core.CoreCoinageAccountBackupObserver
import io.paritytech.polkadotapp.feature_coinage_impl.core.CoreCoinageBackupService
import io.paritytech.polkadotapp.feature_coinage_impl.core.CoreCoinageConfig
import io.paritytech.polkadotapp.feature_coinage_impl.core.CoreCoinageHoldingsUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.core.CoreCoinageOperationStatusUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.core.CoreCoinagePaymentStatusUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.core.CoreCoinageServiceStarter
import io.paritytech.polkadotapp.feature_coinage_impl.core.CoreExternalPaymentPlanner
import io.paritytech.polkadotapp.feature_coinage_impl.core.CoreExternalPaymentService
import io.paritytech.polkadotapp.feature_coinage_impl.core.CoreOnboardingUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.core.CorePrepareCoinageTransferUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.core.CoreTotalBalanceUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore.AccountDataStoreConfigProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore.RemoteConfigAccountDataStoreConfigProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.debug.RealCoinageDebugSettings
import io.paritytech.polkadotapp.feature_coinage_impl.data.storage.DeepRecoveryCompletedStorage
import io.paritytech.polkadotapp.feature_coinage_impl.data.storage.RealDeepRecoveryCompletedStorage
import io.paritytech.polkadotapp.feature_coinage_impl.data.storage.RecyclingStrategyStorage
import io.paritytech.polkadotapp.feature_coinage_impl.data.storage.createRecyclingStrategyStorage
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.RealCoinageRecyclingStrategySettings
import io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase.RealCoinageTestnetFundUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase.RealShareCoinageLogsUseCase
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.CoreCoinageSetup
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
object CoinageStorageModule {
    @Provides
    @Singleton
    fun provideRecyclingStrategyStorage(factory: SingleValueStorageFactory): RecyclingStrategyStorage {
        return factory.createRecyclingStrategyStorage()
    }
}

@Module
@InstallIn(SingletonComponent::class)
interface CoinageFeatureModule {
    @Binds
    fun bindCoinageRecyclingStrategySettings(impl: RealCoinageRecyclingStrategySettings): CoinageRecyclingStrategySettings

    @Binds
    fun bindDeepRecoveryCompletedStorage(impl: RealDeepRecoveryCompletedStorage): DeepRecoveryCompletedStorage

    @Binds
    fun bindAccountDataStoreConfigProvider(impl: RemoteConfigAccountDataStoreConfigProvider): AccountDataStoreConfigProvider

    @Binds
    fun bindCoreCoinageSetup(impl: CoreCoinageConfig): CoreCoinageSetup

    @Binds
    fun bindCoinageAccountBackupObserver(impl: CoreCoinageAccountBackupObserver): CoinageAccountBackupObserver

    @Binds
    fun bindCoinageBackupService(impl: CoreCoinageBackupService): CoinageBackupService

    @Binds
    fun bindCoinageServiceStarter(impl: CoreCoinageServiceStarter): CoinageServiceStarter

    @Binds
    fun bindTotalBalanceUseCase(impl: CoreTotalBalanceUseCase): TotalBalanceUseCase

    @Binds
    fun bindCoinageHoldingsUseCase(impl: CoreCoinageHoldingsUseCase): CoinageHoldingsUseCase

    @Binds
    fun bindPrepareCoinageTransferUseCase(impl: CorePrepareCoinageTransferUseCase): PrepareCoinageTransferUseCase

    @Binds
    fun bindCoinagePaymentStatusUseCase(impl: CoreCoinagePaymentStatusUseCase): CoinagePaymentStatusUseCase

    @Binds
    fun bindClaimReceivedCoinsUseCase(impl: CoreClaimReceivedCoinsUseCase): ClaimReceivedCoinsUseCase

    @Binds
    fun bindCoinageOperationStatusUseCase(impl: CoreCoinageOperationStatusUseCase): CoinageOperationStatusUseCase

    @Binds
    fun bindOnboardingUseCase(impl: CoreOnboardingUseCase): OnboardingUseCase

    @Binds
    fun bindExternalPaymentPlanner(impl: CoreExternalPaymentPlanner): ExternalPaymentPlanner

    @Binds
    fun bindExternalPaymentService(impl: CoreExternalPaymentService): ExternalPaymentService

    @Binds
    fun bindCoinageTestnetFundUseCase(impl: RealCoinageTestnetFundUseCase): CoinageTestnetFundUseCase

    @Binds
    fun bindShareCoinageLogsUseCase(impl: RealShareCoinageLogsUseCase): ShareCoinageLogsUseCase

    @Binds
    @Singleton
    fun bindCoinageDebugSettings(impl: RealCoinageDebugSettings): CoinageDebugSettings
}
