package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import uniffi.truapi.SecretCoreStorageKey
import uniffi.truapi.secretCoreStorageKeyIdentifier

class TrUAPISecretStorageTest {
    private val preferences = mockk<EncryptedPreferences>()
    private val storage = TrUAPISecretStorage(preferences, mockk())
    private val key = SecretCoreStorageKey.StorageEncryptionKey

    @Before
    fun keyName() {
        mockkStatic(::secretCoreStorageKeyIdentifier)
        every { secretCoreStorageKeyIdentifier(key) } returns "core-secret"
    }

    @After
    fun restoreKeyName() = unmockkStatic(::secretCoreStorageKeyIdentifier)

    @Test
    fun `only a missing protected record is absent`() = runTest {
        every { preferences.getDecryptedStringOrThrow("core-secret") } returns null
        assertNull(storage.read(key))
        every { preferences.getDecryptedStringOrThrow("core-secret") } returns ""
        assertTrue(runCatching { storage.read(key) }.isFailure)
        val inaccessible = IllegalStateException("keystore locked")
        every { preferences.getDecryptedStringOrThrow("core-secret") } throws inaccessible
        assertSame(inaccessible, runCatching { storage.read(key) }.exceptionOrNull())
    }

    @Test
    fun `failed committed writes do not fall back to asynchronous preferences`() = runTest {
        val failure = IllegalStateException("commit failed")
        every { preferences.putEncryptedStringCommitted(any(), any()) } throws failure
        assertSame(failure, runCatching { storage.write(key, byteArrayOf(1)) }.exceptionOrNull())
        verify(exactly = 0) { preferences.putEncryptedString(any(), any()) }
    }

    @Test
    fun `host cleanup cannot rotate the shared chat identity`() = runTest {
        assertTrue(runCatching { storage.clear(SecretCoreStorageKey.DeviceEncryptionKey) }.isFailure)
        verify(exactly = 0) { preferences.removeKeyCommitted(any()) }
    }
}
