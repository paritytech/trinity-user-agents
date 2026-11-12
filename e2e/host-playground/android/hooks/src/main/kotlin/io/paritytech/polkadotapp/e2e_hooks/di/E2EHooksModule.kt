package io.paritytech.polkadotapp.e2e_hooks.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.IntoSet
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.e2e_hooks.HostPlaygroundE2EInitializer

@Module
@InstallIn(SingletonComponent::class)
internal interface E2EHooksModule {
    @Binds
    @IntoSet
    fun bindInitializer(impl: HostPlaygroundE2EInitializer): AppInitializer
}
