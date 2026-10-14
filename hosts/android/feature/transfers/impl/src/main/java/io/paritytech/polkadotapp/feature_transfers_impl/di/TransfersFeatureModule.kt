package io.paritytech.polkadotapp.feature_transfers_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_transfers_api.data.repository.SendRecipientRepository
import io.paritytech.polkadotapp.feature_transfers_api.data.type.TokenTransfersTypeRegistry
import io.paritytech.polkadotapp.feature_transfers_api.domain.usecase.PreviousSendRecipientsUseCase
import io.paritytech.polkadotapp.feature_transfers_api.domain.usecase.TestnetFundUseCase
import io.paritytech.polkadotapp.feature_transfers_api.presentation.PreviousPaymentsAddressConverterFactory
import io.paritytech.polkadotapp.feature_transfers_impl.data.repository.RealSendRecipientRepository
import io.paritytech.polkadotapp.feature_transfers_impl.data.type.RealTokenTransfersTypeRegistry
import io.paritytech.polkadotapp.feature_transfers_impl.domain.RealPreviousSendRecipientsUseCase
import io.paritytech.polkadotapp.feature_transfers_impl.domain.RealTestnetFundUseCase
import io.paritytech.polkadotapp.feature_transfers_impl.presentation.RealPreviousPaymentsAddressConverterFactory
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
interface TransfersFeatureApiModule {
    @Binds
    @Singleton
    fun bindTransfersRegistry(implementation: RealTokenTransfersTypeRegistry): TokenTransfersTypeRegistry

    @Binds
    @Singleton
    fun bindPreviousSendRecipientsUseCase(implementation: RealPreviousSendRecipientsUseCase): PreviousSendRecipientsUseCase

    @Binds
    fun bindTopUpBalanceUseCase(implementation: RealTestnetFundUseCase): TestnetFundUseCase

    @Binds
    fun bindSendRecipientRepository(implementation: RealSendRecipientRepository): SendRecipientRepository

    @Binds
    fun bindPreviousPaymentsAddressConverterFactory(implementation: RealPreviousPaymentsAddressConverterFactory): PreviousPaymentsAddressConverterFactory
}
