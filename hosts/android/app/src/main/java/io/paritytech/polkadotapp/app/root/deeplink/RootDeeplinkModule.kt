package io.paritytech.polkadotapp.app.root.deeplink

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler

@Module
@InstallIn(SingletonComponent::class)
internal interface RootDeeplinkModule {
    @Binds
    fun bindDeepLinkHandler(impl: RootDeeplinkHandler): DeepLinkHandler
}
