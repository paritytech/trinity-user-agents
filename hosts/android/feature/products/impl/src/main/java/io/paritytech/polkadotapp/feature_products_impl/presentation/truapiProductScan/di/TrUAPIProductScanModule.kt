package io.paritytech.polkadotapp.feature_products_impl.presentation.truapiProductScan.di

import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.components.ViewModelComponent
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductScanRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIProductScans
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIPrompt
import uniffi.truapi.HostScan

@Module
@InstallIn(ViewModelComponent::class)
class TrUAPIProductScanModule {
    @Provides
    fun provideProductScanPrompt(scans: TrUAPIProductScans): TrUAPIPrompt<ProductScanRequest, HostScan> =
        requireNotNull(scans.current) { "No product scan is open." }
}
