package io.paritytech.polkadotapp.common.data.storage.preferences.encrypted

import android.content.Context
import android.content.SharedPreferences
import io.mockk.every
import io.mockk.justRun
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.mockk.verify
import org.junit.Assert.assertThrows
import org.junit.Test
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.cert.Certificate

class EncryptionUtilTest {
    private val preferences = mockk<SharedPreferences>()
    private val context = mockk<Context> {
        every { getSharedPreferences("key_alias", Context.MODE_PRIVATE) } returns preferences
    }

    @Test
    fun `an empty persisted wrapping cannot generate a replacement encryption key`() {
        every { preferences.getString("secret_key", null) } returns ""
        val cipher = EncryptionUtil(context)
        assertThrows(IllegalStateException::class.java) { cipher.getPrerenceAesKey() }
        verify(exactly = 0) { preferences.edit() }
    }

    @Test
    fun `missing Keystore key cannot replace protection for existing ciphertext`() {
        mockkStatic(KeyStore::class)
        mockkStatic(KeyPairGenerator::class)
        try {
            val keystore = mockk<KeyStore>(relaxed = true) {
                every { getKey("key_alias", null) } returns null
            }
            every { KeyStore.getInstance("AndroidKeyStore") } returns keystore
            every { preferences.getString("secret_key", null) } returns "existing-wrapped-key"
            val cipher = EncryptionUtil(context)
            assertThrows(IllegalStateException::class.java) { cipher.getPrerenceAesKey() }
            verify(exactly = 0) { KeyPairGenerator.getInstance(any<String>(), any<String>()) }
            verify(exactly = 0) { preferences.edit() }
        } finally {
            unmockkStatic(KeyStore::class)
            unmockkStatic(KeyPairGenerator::class)
        }
    }

    @Test
    fun `a failed wrapping commit never exposes or caches a key`() {
        val protection = KeyPairGenerator.getInstance("RSA").apply { initialize(2048) }.generateKeyPair()
        val certificate = mockk<Certificate> {
            every { publicKey } returns protection.public
        }
        val keystore = mockk<KeyStore>(relaxed = true) {
            every { getKey("key_alias", null) } returns protection.private
            every { getCertificate("key_alias") } returns certificate
        }
        var stored: String? = null
        val editor = mockk<SharedPreferences.Editor>()
        every { editor.putString("secret_key", any()) } answers {
            stored = secondArg()
            editor
        }
        every { editor.commit() } returns false
        justRun { editor.apply() }
        every { preferences.getString("secret_key", any()) } answers { stored ?: secondArg() }
        every { preferences.edit() } returns editor
        mockkStatic(KeyStore::class)
        try {
            every { KeyStore.getInstance("AndroidKeyStore") } returns keystore
            val cipher = EncryptionUtil(context)
            repeat(2) {
                assertThrows(IllegalStateException::class.java) { cipher.getPrerenceAesKey() }
            }
            verify(exactly = 2) { editor.commit() }
            verify(exactly = 0) { editor.apply() }
        } finally {
            unmockkStatic(KeyStore::class)
        }
    }
}
