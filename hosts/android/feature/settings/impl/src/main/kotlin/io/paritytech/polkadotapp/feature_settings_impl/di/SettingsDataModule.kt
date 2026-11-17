package io.paritytech.polkadotapp.feature_settings_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_settings_api.domain.docklet.DockletAutoHideSettings
import io.paritytech.polkadotapp.feature_settings_api.domain.language.AppLanguageProvider
import io.paritytech.polkadotapp.feature_settings_impl.data.repository.RealAppLanguageProvider
import io.paritytech.polkadotapp.feature_settings_impl.data.repository.RealDockletAutoHideSettings

@Module
@InstallIn(SingletonComponent::class)
internal interface SettingsDataModule {
    @Binds
    fun bindAppLanguageProvider(impl: RealAppLanguageProvider): AppLanguageProvider

    @Binds
    fun bindDockletAutoHideSettings(impl: RealDockletAutoHideSettings): DockletAutoHideSettings
}
