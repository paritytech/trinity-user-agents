package io.paritytech.polkadotapp.tools_backup_impl.data.processing

import io.novasama.substrate_sdk_android.encrypt.json.copyBytes
import io.novasama.substrate_sdk_android.encrypt.xsalsa20poly1305.SecretBox
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.dropBytes
import io.paritytech.polkadotapp.tools_backup_api.domain.error.CorruptedBackupException
import io.paritytech.polkadotapp.tools_backup_api.domain.model.Backup
import io.paritytech.polkadotapp.tools_backup_impl.data.model.EncryptedBackup
import io.paritytech.polkadotapp.tools_backup_impl.data.model.EncryptionKey
import kotlinx.coroutines.withContext
import org.bouncycastle.crypto.generators.SCrypt
import java.security.SecureRandom
import java.util.Random
import javax.inject.Inject

interface BackupEncryption {
    suspend fun generateNewEncryptionKey(): EncryptionKey

    suspend fun encryptBackup(data: Backup, key: EncryptionKey): Result<EncryptedBackup>

    suspend fun decryptBackup(data: EncryptedBackup, key: EncryptionKey): Result<Backup>
}

class ScryptBackupEncryption @Inject constructor(
    private val coroutineDispatchers: CoroutineDispatchers
) : BackupEncryption {
    private val random: Random = SecureRandom()

    companion object {
        private const val SCRYPT_KEY_SIZE = 32
        private const val SALT_SIZE = 32
        private const val NONCE_SIZE = 24

        private const val N = 16384
        private const val p = 1
        private const val r = 8

        private const val ENCRYPTION_KEY_SIZE = 32
    }

    override suspend fun generateNewEncryptionKey(): EncryptionKey {
        val bytes = ByteArray(ENCRYPTION_KEY_SIZE)
        random.nextBytes(bytes)

        return EncryptionKey(bytes)
    }

    override suspend fun encryptBackup(data: Backup, key: EncryptionKey): Result<EncryptedBackup> {
        return runCatching {
            withContext(coroutineDispatchers.computation) {
                val salt = generateSalt()
                val encryptionKey = generateScryptKey(key.value, salt)
                val plaintext = data.value

                val secretBox = SecretBox(encryptionKey)
                val nonce = secretBox.nonce(plaintext)

                val secret = secretBox.seal(nonce, plaintext)
                val encryptedData = salt + nonce + secret

                EncryptedBackup(encryptedData)
            }
        }
    }

    override suspend fun decryptBackup(data: EncryptedBackup, key: EncryptionKey): Result<Backup> {
        return runCatching {
            withContext(coroutineDispatchers.computation) {
                if (data.value.size < SALT_SIZE + NONCE_SIZE) {
                    throw CorruptedBackupException()
                }

                val salt = data.value.copyBytes(from = 0, size = SALT_SIZE)
                val nonce = data.value.copyBytes(from = SALT_SIZE, size = NONCE_SIZE)
                val encryptedContent = data.value.dropBytes(SALT_SIZE + NONCE_SIZE)

                val encryptionSecret = generateScryptKey(key.value, salt)

                val secret = SecretBox(encryptionSecret).open(nonce, encryptedContent)

                if (secret.isEmpty()) {
                    throw CorruptedBackupException()
                }

                Backup(secret)
            }
        }
    }

    private fun generateScryptKey(password: ByteArray, salt: ByteArray): ByteArray {
        return SCrypt.generate(password, salt, N, r, p, SCRYPT_KEY_SIZE)
    }

    private fun generateSalt(): ByteArray {
        return ByteArray(SALT_SIZE).also {
            random.nextBytes(it)
        }
    }
}
