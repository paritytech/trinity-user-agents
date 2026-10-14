package io.paritytech.polkadotapp.feature_settings_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.android.components.ViewModelComponent
import dagger.hilt.android.scopes.ViewModelScoped
import io.paritytech.polkadotapp.feature_settings_impl.domain.interactors.BackupConflictInteractor
import io.paritytech.polkadotapp.feature_settings_impl.domain.interactors.BackupStatusInteractor
import io.paritytech.polkadotapp.feature_settings_impl.domain.interactors.CurrencyInteractor
import io.paritytech.polkadotapp.feature_settings_impl.domain.interactors.MnemonicRevealInteractor
import io.paritytech.polkadotapp.feature_settings_impl.domain.interactors.RealBackupConflictInteractor
import io.paritytech.polkadotapp.feature_settings_impl.domain.interactors.RealBackupStatusInteractor
import io.paritytech.polkadotapp.feature_settings_impl.domain.interactors.RealCurrencyInteractor
import io.paritytech.polkadotapp.feature_settings_impl.domain.interactors.RealMnemonicRevealInteractor

@Module
@InstallIn(ViewModelComponent::class)
interface SettingsFeatureModule {
    @Binds
    @ViewModelScoped
    fun bindMnemonicRevealInteractor(impl: RealMnemonicRevealInteractor): MnemonicRevealInteractor

    @Binds
    @ViewModelScoped
    fun bindBackupStatusInteractor(impl: RealBackupStatusInteractor): BackupStatusInteractor

    @Binds
    @ViewModelScoped
    fun bindBackupConflictInteractor(impl: RealBackupConflictInteractor): BackupConflictInteractor

    @Binds
    @ViewModelScoped
    fun bindCurrencyInteractor(impl: RealCurrencyInteractor): CurrencyInteractor
}
