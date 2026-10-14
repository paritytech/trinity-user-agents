package io.paritytech.polkadotapp.tools_integrity_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.tools_integrity_impl.data.RealVanillaIntegrityParamsInjector
import io.paritytech.polkadotapp.tools_integrity_impl.data.integrity.IntegrityParamsInjector

@Module
@InstallIn(SingletonComponent::class)
interface VanillaIntegrityModule {

    @Binds
    fun bindIntegrityHeadersInjector(impl: RealVanillaIntegrityParamsInjector): IntegrityParamsInjector
}
