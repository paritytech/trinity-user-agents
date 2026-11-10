package io.paritytech.polkadotapp.chains.di

import com.google.gson.Gson
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.chains.call.MultiChainRuntimeCallsApi
import io.paritytech.polkadotapp.chains.call.MultiChainViewFunctionsApi
import io.paritytech.polkadotapp.chains.call.RealMultiChainRuntimeCallsApi
import io.paritytech.polkadotapp.chains.call.RealMultiChainViewFunctionsApi
import io.paritytech.polkadotapp.chains.extrinsic.visitor.call.api.CallTraversal
import io.paritytech.polkadotapp.chains.extrinsic.visitor.call.impl.RealCallTraversal
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.requests.StorageSharedRequestsBuilderFactory
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.ChainEventsRepositoryFactory
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.DbRuntimeVersionsRepository
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.EventsRepository
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.RemoteEventsRepository
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.RuntimeVersionsRepository
import io.paritytech.polkadotapp.chains.network.rpc.BulkRetriever
import io.paritytech.polkadotapp.chains.network.rpc.RpcCalls
import io.paritytech.polkadotapp.chains.network.updaters.BlockNumberUpdater
import io.paritytech.polkadotapp.chains.network.updaters.BlockTimeUpdater
import io.paritytech.polkadotapp.chains.network.updaters.system.UpdateSystemFactory
import io.paritytech.polkadotapp.chains.repository.ChainStateRepository
import io.paritytech.polkadotapp.chains.repository.RealChainStateRepository
import io.paritytech.polkadotapp.chains.storage.DbStorageCache
import io.paritytech.polkadotapp.chains.storage.PrefsSampledBlockTimeStorage
import io.paritytech.polkadotapp.chains.storage.SampledBlockTimeStorage
import io.paritytech.polkadotapp.chains.storage.StorageCache
import io.paritytech.polkadotapp.chains.storage.source.LocalStorageSource
import io.paritytech.polkadotapp.chains.storage.source.RemoteStorageSource
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.RemoteStorageQueryContextFactory
import io.paritytech.polkadotapp.chains.util.AddressFormatter
import io.paritytech.polkadotapp.chains.util.RealAddressFormatter
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.database.dao.ChainDao
import io.paritytech.polkadotapp.database.dao.StorageDao
import javax.inject.Qualifier
import javax.inject.Singleton

@Qualifier
@Retention(AnnotationRetention.BINARY)
annotation class LocalSourceQualifier

@Qualifier
@Retention(AnnotationRetention.BINARY)
annotation class RemoteSourceQualifier

const val BULK_RETRIEVER_PAGE_SIZE = 1000

@Module
@InstallIn(SingletonComponent::class)
internal class RuntimeModule {
    @Provides
    @Singleton
    fun provideStorageCache(storageDao: StorageDao): StorageCache = DbStorageCache(storageDao)

    @Provides
    @Singleton
    @LocalSourceQualifier
    fun provideLocalStorageSource(
        chainRegistry: ChainRegistry,
        storageCache: StorageCache,
        sharedRequestsBuilderFactory: StorageSharedRequestsBuilderFactory,
        coroutineDispatchers: CoroutineDispatchers,
    ): StorageDataSource =
        LocalStorageSource(chainRegistry, sharedRequestsBuilderFactory, coroutineDispatchers, storageCache)

    @Provides
    @Singleton
    fun provideBulkRetriever(): BulkRetriever {
        return BulkRetriever(BULK_RETRIEVER_PAGE_SIZE)
    }

    @Provides
    @Singleton
    fun provideRemoteStorageContextFactory(
        chainRegistry: ChainRegistry,
        bulkRetriever: BulkRetriever,
        sharedRequestsBuilderFactory: StorageSharedRequestsBuilderFactory,
    ): RemoteStorageQueryContextFactory {
        return RemoteStorageQueryContextFactory(chainRegistry, bulkRetriever, sharedRequestsBuilderFactory)
    }

    @Provides
    @Singleton
    @RemoteSourceQualifier
    fun provideRemoteStorageSource(
        chainRegistry: ChainRegistry,
        sharedRequestsBuilderFactory: StorageSharedRequestsBuilderFactory,
        remoteStorageQueryContextFactory: RemoteStorageQueryContextFactory,
        coroutineDispatchers: CoroutineDispatchers,
    ): StorageDataSource =
        RemoteStorageSource(chainRegistry, sharedRequestsBuilderFactory, remoteStorageQueryContextFactory, coroutineDispatchers)

    @Provides
    @Singleton
    fun provideSubstrateCalls(
        chainRegistry: ChainRegistry,
        multiChainRuntimeCallsApi: MultiChainRuntimeCallsApi,
    ) = RpcCalls(chainRegistry, multiChainRuntimeCallsApi)

    @Provides
    @Singleton
    fun provideRuntimeVersionsRepository(chainDao: ChainDao): RuntimeVersionsRepository = DbRuntimeVersionsRepository(chainDao)

    @Provides
    @Singleton
    fun provideChainEventsRepositoryFactory(
        remoteStorageQueryContextFactory: RemoteStorageQueryContextFactory,
        defaultRpcCalls: RpcCalls,
    ): ChainEventsRepositoryFactory {
        return ChainEventsRepositoryFactory(remoteStorageQueryContextFactory, defaultRpcCalls)
    }

    @Provides
    @Singleton
    fun provideEventsRepository(
        chainRegistry: ChainRegistry,
        chainEventsRepositoryFactory: ChainEventsRepositoryFactory,
    ): EventsRepository = RemoteEventsRepository(chainRegistry, chainEventsRepositoryFactory)

    @Provides
    @Singleton
    fun provideStorageSharedRequestBuilderFactory(chainRegistry: ChainRegistry) = StorageSharedRequestsBuilderFactory(chainRegistry)

    @Provides
    @Singleton
    fun provideMultiChainRuntimeCallsApi(chainRegistry: ChainRegistry): MultiChainRuntimeCallsApi = RealMultiChainRuntimeCallsApi(chainRegistry)

    @Provides
    @Singleton
    fun provideMultiChainViewFunctionsApi(
        multiChainRuntimeCallsApi: MultiChainRuntimeCallsApi
    ): MultiChainViewFunctionsApi = RealMultiChainViewFunctionsApi(multiChainRuntimeCallsApi)

    @Provides
    @Singleton
    fun provideBlockNumberUpdater(chainRegistry: ChainRegistry, storageCache: StorageCache): BlockNumberUpdater {
        return BlockNumberUpdater(chainRegistry, storageCache)
    }

    @Provides
    @Singleton
    fun provideBlockTimeUpdater(
        chainRegistry: ChainRegistry,
        sampledBlockTimeStorage: SampledBlockTimeStorage,
        @RemoteSourceQualifier remoteStorage: StorageDataSource,
    ) = BlockTimeUpdater(chainRegistry, sampledBlockTimeStorage, remoteStorage)

    @Provides
    @Singleton
    fun provideSampledBlockTimeStorage(
        gson: Gson,
        preferences: Preferences,
    ): SampledBlockTimeStorage = PrefsSampledBlockTimeStorage(gson, preferences)

    @Provides
    @Singleton
    fun provideChainStateRepository(
        @LocalSourceQualifier localStorage: StorageDataSource,
        @RemoteSourceQualifier remoteStorage: StorageDataSource,
        chainRegistry: ChainRegistry,
        sampledBlockTimeStorage: SampledBlockTimeStorage,
        coroutineDispatchers: CoroutineDispatchers,
        rpcCalls: RpcCalls
    ): ChainStateRepository = RealChainStateRepository(
        localStorage = localStorage,
        remoteStorage = remoteStorage,
        sampledBlockTimeStorage = sampledBlockTimeStorage,
        chainRegistry = chainRegistry,
        dispatchers = coroutineDispatchers,
        rpcCalls = rpcCalls
    )

    @Provides
    @Singleton
    fun provideUpdateSystemFactory(
        chainRegistry: ChainRegistry,
        dispatchers: CoroutineDispatchers,
        storageSharedRequestsBuilderFactory: StorageSharedRequestsBuilderFactory
    ): UpdateSystemFactory {
        return UpdateSystemFactory(chainRegistry, dispatchers, storageSharedRequestsBuilderFactory)
    }

    @Provides
    @Singleton
    fun provideAddressFormatter(
        chainRegistry: ChainRegistry
    ): AddressFormatter = RealAddressFormatter(chainRegistry)

    @Provides
    @Singleton
    fun defaultCallTraversal(): CallTraversal {
        return RealCallTraversal()
    }
}
