package io.paritytech.polkadotapp.tools_backup_impl.data.processing

import io.paritytech.polkadotapp.common.utils.RealCoroutineDispatchers
import io.paritytech.polkadotapp.tools_backup_api.domain.error.CorruptedBackupException
import io.paritytech.polkadotapp.tools_backup_api.domain.model.Backup
import io.paritytech.polkadotapp.tools_backup_impl.data.model.EncryptedBackup
import io.paritytech.polkadotapp.tools_backup_impl.data.model.EncryptionKey
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertTrue
import org.junit.Test

class ScryptBackupEncryptionTest {
    private val encryption = ScryptBackupEncryption(RealCoroutineDispatchers())

    @Test
    fun `decrypting empty backup fails with CorruptedBackupException`() = runTest {
        val result = encryption.decryptBackup(EncryptedBackup(ByteArray(0)), key(0))

        assertTrue(result.isFailure)
        assertTrue(result.exceptionOrNull() is CorruptedBackupException)
    }

    @Test
    fun `decrypting too short backup fails with CorruptedBackupException`() = runTest {
        val result = encryption.decryptBackup(EncryptedBackup(ByteArray(10)), key(0))

        assertTrue(result.isFailure)
        assertTrue(result.exceptionOrNull() is CorruptedBackupException)
    }

    @Test
    fun `decrypting with a wrong key fails with CorruptedBackupException`() = runTest {
        val keyA = key(1)
        val keyB = key(2)

        val encrypted = encryption.encryptBackup(Backup(ByteArray(64) { it.toByte() }), keyA).getOrNull()
        assertTrue(encrypted != null)

        val result = encryption.decryptBackup(encrypted!!, keyB)

        assertTrue(result.isFailure)
        assertTrue(result.exceptionOrNull() is CorruptedBackupException)
    }

    private fun key(seed: Int) = EncryptionKey(ByteArray(32) { (it + seed).toByte() })
}
