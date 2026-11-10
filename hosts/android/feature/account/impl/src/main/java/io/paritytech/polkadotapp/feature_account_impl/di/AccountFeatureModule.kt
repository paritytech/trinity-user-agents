package io.paritytech.polkadotapp.feature_account_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.Multibinds
import io.paritytech.polkadotapp.chains.network.updaters.Updater
import io.paritytech.polkadotapp.feature_account_api.data.CandidateAccount
import io.paritytech.polkadotapp.feature_account_api.data.WalletAccount
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.sign.AccountBytesSigner
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsFactory
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.BandersnatchSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.newaccount.NewAccountStorage
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.AccountDerivationProvider
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.RingVrfDerivationProvider
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.RingVrfEntropyDeriver
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.AccountDerivationUseCase
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.CreateNewAccountUseCase
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.SharedSecretDerivationUseCase
import io.paritytech.polkadotapp.feature_account_api.presentation.address.converter.ParseAddressConverterFactory
import io.paritytech.polkadotapp.feature_account_api.presentation.address.mixin.AddressInputMixin
import io.paritytech.polkadotapp.feature_account_impl.data.RealAccountRepository
import io.paritytech.polkadotapp.feature_account_impl.data.sign.RealAccountBytesSigner
import io.paritytech.polkadotapp.feature_account_impl.data.storage.accountSecrets.RealAccountSecretsFactory
import io.paritytech.polkadotapp.feature_account_impl.data.storage.accountSecrets.RealAccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_impl.data.storage.accountSecrets.RealBandersnatchSecretsStorage
import io.paritytech.polkadotapp.feature_account_impl.data.storage.newaccount.RealNewAccountStorage
import io.paritytech.polkadotapp.feature_account_impl.data.updaters.RealCandidateAccountUpdateScope
import io.paritytech.polkadotapp.feature_account_impl.data.updaters.RealWalletAccountUpdateScope
import io.paritytech.polkadotapp.feature_account_impl.domain.derivation.RealRingVrfEntropyDeriver
import io.paritytech.polkadotapp.feature_account_impl.domain.usecase.RealAccountDerivationUseCase
import io.paritytech.polkadotapp.feature_account_impl.domain.usecase.RealCreateNewAccountUseCase
import io.paritytech.polkadotapp.feature_account_impl.domain.usecase.RealSharedSecretDerivationUseCase
import io.paritytech.polkadotapp.feature_account_impl.presentation.address.converter.RealParseAddressConverterFactory
import io.paritytech.polkadotapp.feature_account_impl.presentation.address.mixin.AddressInputMixinFactory
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
interface AccountFeatureApiModule {
    @Binds
    fun bindNewAccountStorage(impl: RealNewAccountStorage): NewAccountStorage

    @Binds
    fun bindAccountSecretsStorage(real: RealAccountSecretsStorage): AccountSecretsStorage

    @Binds
    fun bindBandersnatchSecretsStorage(real: RealBandersnatchSecretsStorage): BandersnatchSecretsStorage

    @Binds
    fun bindRingVrfEntropyDeriver(real: RealRingVrfEntropyDeriver): RingVrfEntropyDeriver

    @Binds
    fun bindAccountRepository(real: RealAccountRepository): AccountRepository

    @Binds
    @WalletAccount
    fun bindWalletAccountUpdateScope(real: RealWalletAccountUpdateScope): Updater.NoChainScope<MetaAccount>

    @Binds
    @CandidateAccount
    fun bindCandidateAccountUpdateScope(real: RealCandidateAccountUpdateScope): Updater.NoChainScope<MetaAccount>

    @Binds
    fun bindAccountSecretsFactory(impl: RealAccountSecretsFactory): AccountSecretsFactory

    @Binds
    fun bindAccountBytesSigner(impl: RealAccountBytesSigner): AccountBytesSigner

    @Binds
    @Singleton
    fun bindAddressInputMixinFactory(impl: AddressInputMixinFactory): AddressInputMixin.Factory

    @Binds
    @Singleton
    fun bindParseAddressConverterFactory(impl: RealParseAddressConverterFactory): ParseAddressConverterFactory

    @Binds
    fun bindChatSecretsUseCase(impl: RealSharedSecretDerivationUseCase): SharedSecretDerivationUseCase

    @Binds
    fun bindAccountDerivationUseCase(impl: RealAccountDerivationUseCase): AccountDerivationUseCase

    @Binds
    fun bindCreateNewAccountUseCase(impl: RealCreateNewAccountUseCase): CreateNewAccountUseCase

    @Multibinds
    fun accountDerivationProviders(): Map<MetaAccount.Purpose, AccountDerivationProvider>

    @Multibinds
    fun ringVrfDerivationProviders(): Map<MetaAccount.Purpose, RingVrfDerivationProvider>
}
