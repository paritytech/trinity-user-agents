package io.paritytech.polkadotapp.feature_pgas_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_pgas_api.domain.PgasChainAssetProvider
import io.paritytech.polkadotapp.feature_pgas_api.domain.PgasClaimer
import io.paritytech.polkadotapp.feature_pgas_impl.data.RealPgasChainAssetProvider
import io.paritytech.polkadotapp.feature_pgas_impl.data.RealPgasClaimer
import io.paritytech.polkadotapp.feature_pgas_impl.data.repository.PgasRepository
import io.paritytech.polkadotapp.feature_pgas_impl.data.repository.RealPgasRepository
import io.paritytech.polkadotapp.feature_pgas_impl.data.signer.origins.PgasOrigins
import io.paritytech.polkadotapp.feature_pgas_impl.data.signer.origins.RealPgasOrigins

@Module
@InstallIn(SingletonComponent::class)
interface PgasFeatureApiModule {
    @Binds
    fun bindPgasClaimer(impl: RealPgasClaimer): PgasClaimer

    @Binds
    fun bindPgasRepository(impl: RealPgasRepository): PgasRepository

    @Binds
    fun bindPgasOrigins(impl: RealPgasOrigins): PgasOrigins

    @Binds
    fun bindPgasChainAssetProvider(impl: RealPgasChainAssetProvider): PgasChainAssetProvider

    @Binds
    fun bindPgasClaimSpec(impl: RealPgasRepository): io.paritytech.polkadotapp.feature_pgas_api.domain.PgasClaimSpec
}
