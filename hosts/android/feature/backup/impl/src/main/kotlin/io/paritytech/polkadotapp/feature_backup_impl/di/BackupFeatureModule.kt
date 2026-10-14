package io.paritytech.polkadotapp.feature_backup_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.IntoSet
import io.paritytech.polkadotapp.common.presentation.tabs.TabWarningProvider
import io.paritytech.polkadotapp.feature_backup_api.domain.usecase.CreateNewAccountAndTryBackupUseCase
import io.paritytech.polkadotapp.feature_backup_api.domain.usecase.ImportAccountFromBackupUseCase
import io.paritytech.polkadotapp.feature_backup_impl.ManualMnemonicInteractor
import io.paritytech.polkadotapp.feature_backup_impl.RealManualMnemonicInteractor
import io.paritytech.polkadotapp.feature_backup_impl.domain.BackupTabWarningProvider
import io.paritytech.polkadotapp.feature_backup_impl.domain.usecase.RealCreateNewAccountAndTryBackupUseCase
import io.paritytech.polkadotapp.feature_backup_impl.domain.usecase.RealImportAccountFromBackupUseCase

@Module
@InstallIn(SingletonComponent::class)
interface BackupFeatureModule {
    @Binds
    fun bindManualMnemonicInteractor(impl: RealManualMnemonicInteractor): ManualMnemonicInteractor

    @Binds
    fun bindImportAccountFromBackupUseCase(impl: RealImportAccountFromBackupUseCase): ImportAccountFromBackupUseCase

    @Binds
    @IntoSet
    fun bindBackupTabWarningProvider(impl: BackupTabWarningProvider): TabWarningProvider

    @Binds
    fun bindCreateNewAccountAndTryBackupUseCase(impl: RealCreateNewAccountAndTryBackupUseCase): CreateNewAccountAndTryBackupUseCase
}
