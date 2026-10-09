package io.paritytech.polkadotapp.feature_products_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.IntoSet
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.e2e.E2ERuntimeMarkers
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.message.ProductMessageRenderObserver

@Module
@InstallIn(SingletonComponent::class)
interface E2EModule {
    @Binds
    @IntoSet
    fun bindRenderMarker(impl: E2ERuntimeMarkers): ProductMessageRenderObserver
}
