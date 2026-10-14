package io.paritytech.polkadotapp.feature_settings_impl.domain.interactors

import io.novasama.substrate_sdk_android.encrypt.mnemonic.Mnemonic
import io.paritytech.polkadotapp.common.domain.errors.BackupDecryptionKeyNotFoundException
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.test_shared.whenever
import io.paritytech.polkadotapp.tools_authentication_api.domain.BiometricsService
import io.paritytech.polkadotapp.tools_backup_api.domain.BackupService
import io.paritytech.polkadotapp.tools_backup_api.domain.error.CorruptedBackupException
import io.paritytech.polkadotapp.tools_backup_api.domain.model.Backup
import io.paritytech.polkadotapp.tools_backup_api.domain.model.RestorableBackup
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import org.mockito.Mockito.mock

class BackupStatusInteractorTest {
    private val backupService: BackupService = mock()
    private val biometricsService: BiometricsService = mock()
    private val accountRepository: AccountRepository = mock()
    private val accountSecretsStorage: AccountSecretsStorage = mock()

    private val interactor = RealBackupStatusInteractor(
        backupService = backupService,
        biometricsService = biometricsService,
        accountRepository = accountRepository,
        accountSecretsStorage = accountSecretsStorage,
        createAndSaveBackupFromMnemonicUseCase = mock(),
    )

    @Before
    fun setUp() = runBlocking<Unit> {
        withWalletAccount()
    }

    @Test
    fun `returns NoAccess when backup download fails`() = runBlocking<Unit> {
        withBackupDownloadFailure()

        assertEquals(BackupState.NoAccess, interactor.resolveBackupState())
    }

    @Test
    fun `returns None when no backup exists`() = runBlocking<Unit> {
        withNoBackup()

        assertEquals(BackupState.None, interactor.resolveBackupState())
    }

    @Test
    fun `returns Available when decrypted entropy matches account passphrase`() = runBlocking<Unit> {
        withDecryptedBackup(BACKUP_ENTROPY)
        withAccountPassphrase(BACKUP_ENTROPY)

        assertEquals(BackupState.Available(CREATED_AT), interactor.resolveBackupState())
    }

    @Test
    fun `returns Conflict when decrypted entropy differs from account passphrase`() = runBlocking<Unit> {
        withDecryptedBackup(BACKUP_ENTROPY)
        withAccountPassphrase(OTHER_ENTROPY)

        assertEquals(BackupState.Conflict(CREATED_AT), interactor.resolveBackupState())
    }

    @Test
    fun `returns Available when decrypted but account passphrase is missing`() = runBlocking<Unit> {
        withDecryptedBackup(BACKUP_ENTROPY)
        withNoAccountPassphrase()

        assertEquals(BackupState.Available(CREATED_AT), interactor.resolveBackupState())
    }

    @Test
    fun `returns Corrupted when decryption fails with CorruptedBackupException`() = runBlocking<Unit> {
        withDecryptionFailure(CorruptedBackupException())

        assertEquals(BackupState.Corrupted, interactor.resolveBackupState())
    }

    @Test
    fun `returns Available when decryption fails with missing decryption key`() = runBlocking<Unit> {
        withDecryptionFailure(BackupDecryptionKeyNotFoundException())

        assertEquals(BackupState.Available(CREATED_AT), interactor.resolveBackupState())
    }

    @Test
    fun `returns NoAccess when restore fails with unexpected error`() = runBlocking<Unit> {
        withDecryptionFailure(RuntimeException("unexpected"))

        assertEquals(BackupState.NoAccess, interactor.resolveBackupState())
    }

    private suspend fun withWalletAccount() {
        val account: MetaAccount = mock()
        whenever(account.id).thenReturn(META_ID)
        whenever(accountRepository.getWalletAccount()).thenReturn(account)
    }

    private suspend fun withBackupDownloadFailure() {
        whenever(backupService.getRestorableBackup()).thenReturn(Result.failure(RuntimeException("network")))
    }

    private suspend fun withNoBackup() {
        whenever(backupService.getRestorableBackup()).thenReturn(Result.success<RestorableBackup?>(null))
    }

    private suspend fun withDecryptedBackup(entropy: ByteArray) {
        withRestorableBackup(Result.success(Backup(entropy)))
    }

    private suspend fun withDecryptionFailure(error: Throwable) {
        withRestorableBackup(Result.failure(error))
    }

    private suspend fun withRestorableBackup(restoreResult: Result<Backup>) {
        val restorable: RestorableBackup = mock()
        whenever(restorable.createdAt).thenReturn(CREATED_AT)
        whenever(restorable.restore()).thenReturn(restoreResult)
        whenever(backupService.getRestorableBackup()).thenReturn(Result.success(restorable))
    }

    private suspend fun withNoAccountPassphrase() {
        whenever(accountSecretsStorage.getMetaAccountPassphrase(META_ID)).thenReturn(null)
    }

    private suspend fun withAccountPassphrase(entropy: ByteArray) {
        val mnemonic: Mnemonic = mock()
        whenever(mnemonic.entropy).thenReturn(entropy)
        whenever(accountSecretsStorage.getMetaAccountPassphrase(META_ID)).thenReturn(mnemonic)
    }

    private companion object {
        const val META_ID = 1L
        const val CREATED_AT = 1_700_000_000_000L
        val BACKUP_ENTROPY = ByteArray(32) { it.toByte() }
        val OTHER_ENTROPY = ByteArray(32) { (it + 1).toByte() }
    }
}
