package io.paritytech.polkadotapp.tools_backup_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.tools_backup_api.data.BackupExistsStorage
import io.paritytech.polkadotapp.tools_backup_api.domain.BackupService
import io.paritytech.polkadotapp.tools_backup_api.domain.usecase.CreateAndSaveBackupFromMnemonicUseCase
import io.paritytech.polkadotapp.tools_backup_impl.data.processing.BackupEncryption
import io.paritytech.polkadotapp.tools_backup_impl.data.processing.ScryptBackupEncryption
import io.paritytech.polkadotapp.tools_backup_impl.data.storage.PreferencesBackupExistsStorage
import io.paritytech.polkadotapp.tools_backup_impl.data.store.backup.EncryptedBackupStore
import io.paritytech.polkadotapp.tools_backup_impl.data.store.backup.GoogleDriveBackupStorage
import io.paritytech.polkadotapp.tools_backup_impl.data.store.key.BackupEncryptionKeyStore
import io.paritytech.polkadotapp.tools_backup_impl.data.store.key.PasskeysBackupEncryptionKeyStore
import io.paritytech.polkadotapp.tools_backup_impl.domain.RealCloudBackupService
import io.paritytech.polkadotapp.tools_backup_impl.domain.usecase.RealCreateAndSaveBackupFromMnemonicUseCase
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
interface BackupModule {
    @Binds
    @Singleton
    fun bindBackupEncryption(impl: ScryptBackupEncryption): BackupEncryption

    @Binds
    @Singleton
    fun bindBackupEncryptionKeyStore(impl: PasskeysBackupEncryptionKeyStore): BackupEncryptionKeyStore

    @Binds
    @Singleton
    fun bindEncryptedBackupStore(impl: GoogleDriveBackupStorage): EncryptedBackupStore

    @Binds
    @Singleton
    fun bindBackupService(impl: RealCloudBackupService): BackupService

    @Binds
    @Singleton
    fun bindBackupExistsStorage(impl: PreferencesBackupExistsStorage): BackupExistsStorage

    @Binds
    fun bindCreateBackupFromMnemonicUseCase(impl: RealCreateAndSaveBackupFromMnemonicUseCase): CreateAndSaveBackupFromMnemonicUseCase
}
