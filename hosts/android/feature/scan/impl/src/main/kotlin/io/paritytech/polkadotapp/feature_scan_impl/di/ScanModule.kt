package io.paritytech.polkadotapp.feature_scan_impl.di

import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.android.components.ViewModelComponent
import dagger.multibindings.Multibinds
import io.paritytech.polkadotapp.feature_scan_api.domain.ScanContentParser

@Module
@InstallIn(ViewModelComponent::class)
interface ScanModule {
    @Multibinds
    fun scanContentParsers(): Set<ScanContentParser>
}
