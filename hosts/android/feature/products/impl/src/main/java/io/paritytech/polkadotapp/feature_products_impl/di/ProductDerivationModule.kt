package io.paritytech.polkadotapp.feature_products_impl.di

import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.IntoMap
import io.paritytech.polkadotapp.feature_account_api.di.AccountPurposeKey
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.AccountDerivationProvider
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.RingVrfDerivationProvider
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import io.paritytech.polkadotapp.feature_products_impl.domain.derivation.ReservedProductAccountDerivationProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.derivation.ReservedRingVrfDerivationProvider

private val FULL_PERSONHOOD_INDEX = DerivationIndex32.fromUInt(0u)
private val LIGHT_PERSONHOOD_INDEX = DerivationIndex32.fromUInt(1u)

@Module
@InstallIn(SingletonComponent::class)
internal object ProductDerivationModule {
    @Provides
    @IntoMap
    @AccountPurposeKey(MetaAccount.Purpose.WALLET)
    fun provideWalletAccountDerivation(dotNsTldProvider: DotNsTldProvider): AccountDerivationProvider {
        return ReservedProductAccountDerivationProvider(dotNsTldProvider, ReservedProductIds::lightPersonIdentity)
    }

    @Provides
    @IntoMap
    @AccountPurposeKey(MetaAccount.Purpose.DEPOSIT)
    fun provideDepositAccountDerivation(dotNsTldProvider: DotNsTldProvider): AccountDerivationProvider {
        return ReservedProductAccountDerivationProvider(dotNsTldProvider, ReservedProductIds::funding)
    }

    @Provides
    @IntoMap
    @AccountPurposeKey(MetaAccount.Purpose.CANDIDATE)
    fun provideCandidateAccountDerivation(dotNsTldProvider: DotNsTldProvider): AccountDerivationProvider {
        return ReservedProductAccountDerivationProvider(dotNsTldProvider, ReservedProductIds::game)
    }

    @Provides
    @IntoMap
    @AccountPurposeKey(MetaAccount.Purpose.CANDIDATE)
    fun provideFullPersonhoodRingVrfDerivation(dotNsTldProvider: DotNsTldProvider): RingVrfDerivationProvider {
        return ReservedRingVrfDerivationProvider(dotNsTldProvider, ReservedProductIds::personhood, FULL_PERSONHOOD_INDEX)
    }

    @Provides
    @IntoMap
    @AccountPurposeKey(MetaAccount.Purpose.WALLET)
    fun provideLightPersonhoodRingVrfDerivation(dotNsTldProvider: DotNsTldProvider): RingVrfDerivationProvider {
        return ReservedRingVrfDerivationProvider(dotNsTldProvider, ReservedProductIds::personhood, LIGHT_PERSONHOOD_INDEX)
    }
}
