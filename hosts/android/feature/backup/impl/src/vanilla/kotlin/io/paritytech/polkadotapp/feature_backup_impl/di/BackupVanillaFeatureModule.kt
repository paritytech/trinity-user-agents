package io.paritytech.polkadotapp.feature_backup_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_backup_api.domain.usecase.TryRecoverFromBackupAndCreateAccountUseCase
import io.paritytech.polkadotapp.feature_backup_impl.domain.RealVanillaTryRecoverFromBackupAndCreateAccountUseCase

@Module
@InstallIn(SingletonComponent::class)
interface BackupVanillaFeatureModule {

    @Binds
    fun bindTryRecoverFromBackupOrCreateAccountUseCase(impl: RealVanillaTryRecoverFromBackupAndCreateAccountUseCase): TryRecoverFromBackupAndCreateAccountUseCase

}
