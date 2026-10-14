package io.paritytech.polkadotapp.app.root.deeplink.product

import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.IntoSet
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.feature_scan_api.domain.DeeplinkScanContentParser
import io.paritytech.polkadotapp.feature_scan_api.domain.ScanContentParser

@Module
@InstallIn(SingletonComponent::class)
internal interface ProductSpaDeeplinkModule {
    @Binds
    @IntoSet
    fun bindProductSpaDeepLinkHandler(impl: ProductSpaDeepLinkHandler): DeepLinkHandler

    companion object {
        @Provides
        @IntoSet
        fun provideProductSpaScanContentParser(
            handler: ProductSpaDeepLinkHandler,
        ): ScanContentParser = DeeplinkScanContentParser(handler)
    }
}
