package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.HostStorage
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptionUtil
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import uniffi.truapi.HostLocalStorageReadException
import java.io.File
import java.security.MessageDigest
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.spec.GCMParameterSpec
import javax.inject.Inject

/** Seals product storage values with the app's preference key: AES-GCM, a fresh nonce first. */
class ProductStorageCipher @Inject constructor(private val encryptionUtil: EncryptionUtil) {
    private val random = SecureRandom()

    fun seal(value: ByteArray): ByteArray {
        val nonce = ByteArray(NONCE_BYTES).also(random::nextBytes)
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, encryptionUtil.getPrerenceAesKey(), GCMParameterSpec(TAG_BITS, nonce))
        return nonce + cipher.doFinal(value)
    }

    /** Null for bytes this key did not seal. */
    fun open(sealed: ByteArray): ByteArray? = runCatching {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.DECRYPT_MODE, encryptionUtil.getPrerenceAesKey(), GCMParameterSpec(TAG_BITS, sealed, 0, NONCE_BYTES))
        cipher.doFinal(sealed, NONCE_BYTES, sealed.size - NONCE_BYTES)
    }.getOrNull()

    private companion object {
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val NONCE_BYTES = 12
        const val TAG_BITS = 128
    }
}

/**
 * The files of product storage (HA3b): one directory per product, one sealed file per key, in the app's no-backup
 * storage. Keeps how many bytes each product uses, so a write past the budget fails without listing the directory.
 */
class ProductStorageFiles(
    private val root: File,
    private val cipher: ProductStorageCipher,
    private val legacy: EncryptedPreferences,
    private val budgetBytes: Long,
) {
    private val usedBytes = mutableMapOf<String, Long>()

    fun storage(productId: String): HostStorage = FileHostStorage(productId)

    private fun directoryOf(productId: String) = File(root, sha256Hex(normalizeProductId(productId)))

    private fun fileOf(productId: String, key: String) = File(directoryOf(productId), sha256Hex(key))

    private fun used(directory: File): Long =
        usedBytes.getOrPut(directory.name) { directory.listFiles()?.sumOf { it.length() } ?: 0 }

    private inner class FileHostStorage(private val productId: String) : HostStorage {
        // Values an earlier build kept in encrypted preferences move to files as they are read.
        private val earlier = EncryptedHostStorage(legacy, productId)
        private val normalizedProductId = normalizeProductId(productId)

        override suspend fun read(key: String): ByteArray? {
            val owner = productStorageKeyOwner(key)?.takeUnless { it == normalizedProductId } ?: productId
            val file = fileOf(owner, key)
            if (file.exists()) return cipher.open(file.readBytes())
            val value = earlier.read(key) ?: return null
            if (owner == productId) runCancellableCatching { write(key, value) }
            return value
        }

        override suspend fun write(key: String, value: ByteArray) {
            val file = fileOf(productId, key)
            val sealed = cipher.seal(value)
            synchronized(this@ProductStorageFiles) {
                val directory = file.parentFile ?: throw HostLocalStorageReadException.Unknown("no directory for $key")
                val used = used(directory) - file.length()
                if (used + sealed.size > budgetBytes) throw HostLocalStorageReadException.Full()
                directory.mkdirs()
                val partial = File(directory, "${file.name}.partial")
                partial.writeBytes(sealed)
                if (!partial.renameTo(file)) throw HostLocalStorageReadException.Unknown("failed to persist $key")
                usedBytes[directory.name] = used + sealed.size
            }
            earlier.clear(key)
        }

        override suspend fun clear(key: String) {
            val file = fileOf(productId, key)
            synchronized(this@ProductStorageFiles) {
                val directory = file.parentFile ?: return@synchronized
                val used = used(directory)
                val length = file.length()
                if (file.delete()) usedBytes[directory.name] = used - length
            }
            earlier.clear(key)
        }
    }
}

private fun sha256Hex(value: String): String =
    MessageDigest.getInstance("SHA-256").digest(value.encodeToByteArray()).joinToString("") { "%02x".format(it) }
