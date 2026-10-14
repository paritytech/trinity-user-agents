package io.paritytech.polkadotapp.feature_backup_impl.backupFound.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.android.components.ViewModelComponent
import dagger.hilt.android.scopes.ViewModelScoped
import io.paritytech.polkadotapp.feature_backup_impl.backupFound.domain.BackupFoundInteractor
import io.paritytech.polkadotapp.feature_backup_impl.backupFound.domain.RealBackupFoundInteractor

@Module
@InstallIn(ViewModelComponent::class)
interface BackupFoundModule {
    @Binds
    @ViewModelScoped
    fun bindBackupFoundInteractor(impl: RealBackupFoundInteractor): BackupFoundInteractor
}
