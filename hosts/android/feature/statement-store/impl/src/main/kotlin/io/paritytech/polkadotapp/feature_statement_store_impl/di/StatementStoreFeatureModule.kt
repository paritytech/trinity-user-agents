package io.paritytech.polkadotapp.feature_statement_store_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementRequestDecoder
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementStoreService
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_api.domain.CommunicationSessionCreator
import io.paritytech.polkadotapp.feature_statement_store_api.domain.OurDeviceKeypairProvider
import io.paritytech.polkadotapp.feature_statement_store_api.domain.StatementStoreMessageProver
import io.paritytech.polkadotapp.feature_statement_store_api.domain.StatementStorePeer
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlotAllocator
import io.paritytech.polkadotapp.feature_statement_store_impl.data.RealStatementRequestDecoder
import io.paritytech.polkadotapp.feature_statement_store_impl.data.RealStatementStorePeer
import io.paritytech.polkadotapp.feature_statement_store_impl.data.RealStatementStoreService
import io.paritytech.polkadotapp.feature_statement_store_impl.data.encryption.CommunicationEncryptionFactory
import io.paritytech.polkadotapp.feature_statement_store_impl.data.repository.RealStatementStoreSlotAllocationRepository
import io.paritytech.polkadotapp.feature_statement_store_impl.data.repository.RealStatementStoreSlotRepository
import io.paritytech.polkadotapp.feature_statement_store_impl.data.repository.StatementStoreSlotAllocationRepository
import io.paritytech.polkadotapp.feature_statement_store_impl.data.repository.StatementStoreSlotRepository
import io.paritytech.polkadotapp.feature_statement_store_impl.data.signer.origins.RealStatementStoreOrigins
import io.paritytech.polkadotapp.feature_statement_store_impl.data.signer.origins.StatementStoreOrigins
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.RealOurDeviceKeypairProvider
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.RealStatementStoreMessageProverFactory
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.CommunicationSessionCreatorFactory
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.codec.IncomingTopicsProvider
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.codec.RealIncomingTopicsProviderFactory
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator.CurrentPeriodProvider
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator.RealCurrentPeriodProvider
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator.RealStatementStoreSlotAllocator
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator.RealStatementStoreSlotRenewer
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator.StatementStoreSlotRenewer
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
interface StatementStoreFeatureApiModule {
    @Binds
    fun bindStatementStoreMessageProverFactory(impl: RealStatementStoreMessageProverFactory): StatementStoreMessageProver.Factory

    @Binds
    fun bindCommunicationSessionCreatorFactory(impl: CommunicationSessionCreatorFactory): CommunicationSessionCreator.Factory

    @Binds
    @Singleton
    fun bindStatementStoreService(impl: RealStatementStoreService): StatementStoreService

    @Binds
    fun bindStatementStorePeer(impl: RealStatementStorePeer): StatementStorePeer

    @Binds
    @Singleton
    fun bindCommunicationEncryptionFactory(impl: CommunicationEncryptionFactory): CommunicationEncryption.Factory

    @Binds
    fun bindStatementStoreSlotAllocator(impl: RealStatementStoreSlotAllocator): StatementStoreSlotAllocator

    @Binds
    @Singleton
    fun bindStatementStoreSlotRepository(impl: RealStatementStoreSlotRepository): StatementStoreSlotRepository

    @Binds
    @Singleton
    fun bindStatementStoreSlotAllocationRepository(impl: RealStatementStoreSlotAllocationRepository): StatementStoreSlotAllocationRepository

    @Binds
    fun bindStatementStoreSlotRenewer(impl: RealStatementStoreSlotRenewer): StatementStoreSlotRenewer

    @Binds
    @Singleton
    fun bindCurrentPeriodProvider(impl: RealCurrentPeriodProvider): CurrentPeriodProvider

    @Binds
    fun bindStatementStoreOrigins(impl: RealStatementStoreOrigins): StatementStoreOrigins

    @Binds
    fun bindStatementRequestDecoder(impl: RealStatementRequestDecoder): StatementRequestDecoder

    @Binds
    fun bindIncomingTopicsProviderFactory(impl: RealIncomingTopicsProviderFactory): IncomingTopicsProvider.Factory

    @Binds
    @Singleton
    fun bindOurDeviceKeypairProvider(impl: RealOurDeviceKeypairProvider): OurDeviceKeypairProvider
}
