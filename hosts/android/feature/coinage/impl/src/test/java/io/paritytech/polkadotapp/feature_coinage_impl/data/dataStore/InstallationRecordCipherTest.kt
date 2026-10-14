package io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.domain.model.hexToDataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_impl.TEST_INSTALLATION
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Pinned against values computed outside this codebase (Python `hashlib.blake2b`, Node `chacha20-poly1305`), so a
 * change here is a change to what iOS has to reproduce, not a refactor.
 */
class InstallationRecordCipherTest {
    private val cipher = InstallationRecordCipher()

    private val key = deriveDataStoreEncryptionKey(SR25519_SECRET)

    @Test
    fun `the encryption key is the context keyed by the whole 64-byte secret`() {
        assertEquals(ENCRYPTION_KEY, key.bytes.value.toHexString(withPrefix = true))
    }

    @Test
    fun `a sealed installation is the pinned record`() {
        assertEquals(RECORD, cipher.seal(TEST_INSTALLATION, key).toString())
    }

    @Test
    fun `sealing the same installation twice gives the same bytes`() {
        assertEquals(cipher.seal(TEST_INSTALLATION, key), cipher.seal(TEST_INSTALLATION, key))
    }

    @Test
    fun `a sealed record opens back to its installation`() {
        assertEquals(TEST_INSTALLATION, cipher.open(cipher.seal(TEST_INSTALLATION, key), key))
    }

    @Test
    fun `a record sealed with a nonce of its own still opens`() {
        assertEquals(TEST_INSTALLATION, cipher.open(RECORD_WITH_ANOTHER_NONCE.hexToDataByteArray(), key))
    }

    @Test
    fun `a tampered record does not open`() {
        val tampered = RECORD.fromHex().also { it[20] = (it[20].toInt() xor 1).toByte() }

        assertNull(cipher.open(tampered.toDataByteArray(), key))
    }

    @Test
    fun `a record sealed under another seed does not open`() {
        val otherKey = deriveDataStoreEncryptionKey(ByteArray(64) { 7 })

        assertNull(cipher.open(cipher.seal(TEST_INSTALLATION, otherKey), key))
    }

    @Test
    fun `bytes of the wrong length are not a record`() {
        assertNull(cipher.open(byteArrayOf(1, 2, 3).toDataByteArray(), key))
    }

    private companion object {
        val SR25519_SECRET = ByteArray(64) { it.toByte() }

        const val ENCRYPTION_KEY = "0xfc7bd73006ab990f9649eebec3aab66023af1f064d33a694d4d7a4f3bef86748"

        // nonce = blake2b256(key = blake2b256(key = encryptionKey, "nonce-key"), "nonce" || installation)[0..12]
        const val RECORD = "0x2cb9ac950a3a69cfa00480e7e221d461069884d050d21b2ba1c05e0e9bd08b3fe0edf24784976d3bd6e5652bb755" +
            "ca90d48906c10e655eb8f3acc4c6"

        const val RECORD_WITH_ANOTHER_NONCE = "0x000102030405060708090a0bcdc58e6066449725c690c637add5e75d5fae9ea0e7c5" +
            "df58e99ac4e0923ba664ba2d2aa75adc69e254a3e31eb2df5595"
    }
}
