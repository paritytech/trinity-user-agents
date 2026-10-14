package io.paritytech.polkadotapp.feature_transaction_storage_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_transaction_storage_api.domain.TransactionStorageAuthorizationUpdaterFactory
import io.paritytech.polkadotapp.feature_transaction_storage_api.domain.TransactionStorageRepository
import io.paritytech.polkadotapp.feature_transaction_storage_api.domain.TransactionStorageService
import io.paritytech.polkadotapp.feature_transaction_storage_api.domain.slotAllocator.TransactionStorageSlotAllocator
import io.paritytech.polkadotapp.feature_transaction_storage_impl.data.RealTransactionStorageService
import io.paritytech.polkadotapp.feature_transaction_storage_impl.data.repository.LongTermStorageSlotRepository
import io.paritytech.polkadotapp.feature_transaction_storage_impl.data.repository.RealLongTermStorageSlotRepository
import io.paritytech.polkadotapp.feature_transaction_storage_impl.data.repository.RealTransactionStorageRepository
import io.paritytech.polkadotapp.feature_transaction_storage_impl.data.signer.origins.RealTransactionStorageOrigins
import io.paritytech.polkadotapp.feature_transaction_storage_impl.data.signer.origins.TransactionStorageOrigins
import io.paritytech.polkadotapp.feature_transaction_storage_impl.data.updater.RealTransactionStorageAuthorizationUpdaterFactory
import io.paritytech.polkadotapp.feature_transaction_storage_impl.domain.slotAllocator.RealTransactionStorageSlotAllocator
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
interface TransactionStorageFeatureApiModule {
    @Binds
    @Singleton
    fun bindTransactionStorageService(impl: RealTransactionStorageService): TransactionStorageService

    @Binds
    fun bindTransactionStorageSlotAllocator(impl: RealTransactionStorageSlotAllocator): TransactionStorageSlotAllocator

    @Binds
    @Singleton
    fun bindLongTermStorageSlotRepository(impl: RealLongTermStorageSlotRepository): LongTermStorageSlotRepository

    @Binds
    fun bindTransactionStorageOrigins(impl: RealTransactionStorageOrigins): TransactionStorageOrigins

    @Binds
    @Singleton
    fun bindTransactionStorageRepository(impl: RealTransactionStorageRepository): TransactionStorageRepository

    @Binds
    @Singleton
    fun bindTransactionStorageAuthorizationUpdaterFactory(
        impl: RealTransactionStorageAuthorizationUpdaterFactory
    ): TransactionStorageAuthorizationUpdaterFactory
}
