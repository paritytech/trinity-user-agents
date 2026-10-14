package io.paritytech.polkadotapp.feature_settings_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_settings_api.domain.language.AppLanguageProvider
import io.paritytech.polkadotapp.feature_settings_impl.data.repository.RealAppLanguageProvider

@Module
@InstallIn(SingletonComponent::class)
internal interface SettingsDataModule {
    @Binds
    fun bindAppLanguageProvider(impl: RealAppLanguageProvider): AppLanguageProvider
}
