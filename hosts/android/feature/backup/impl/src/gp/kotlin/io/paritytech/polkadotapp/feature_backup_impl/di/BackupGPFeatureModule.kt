package io.paritytech.polkadotapp.feature_backup_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_backup_api.domain.usecase.TryRecoverFromBackupAndCreateAccountUseCase
import io.paritytech.polkadotapp.feature_backup_impl.domain.RealGPTryRecoverFromBackupAndCreateAccountUseCase

@Module
@InstallIn(SingletonComponent::class)
interface BackupGPFeatureModule {

    @Binds
    fun bindTryRecoverFromBackupOrCreateAccountUseCase(impl: RealGPTryRecoverFromBackupAndCreateAccountUseCase): TryRecoverFromBackupAndCreateAccountUseCase

}
