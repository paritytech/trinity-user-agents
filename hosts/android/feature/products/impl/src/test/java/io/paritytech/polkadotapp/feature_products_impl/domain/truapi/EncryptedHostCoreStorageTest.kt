package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.HostRejection

class EncryptedHostCoreStorageTest {
    private val encrypted = mockk<EncryptedPreferences>()
    private val backing = mockk<Preferences> { every { edit().commit() } returns true }
    private val storage = EncryptedHostCoreStorage(encrypted, backing)
    private val key = byteArrayOf(13)

    @Test
    fun `a failed commit cannot expose the value still visible in memory`() = runTest {
        every { backing.edit().commit() } returnsMany listOf(true, false)
        every { encrypted.putEncryptedStringCommitted(any(), any()) } throws IllegalStateException("disk full")
        every { encrypted.getDecryptedStringOrThrow(any()) } returns "v1:01"
        val write = runCatching { storage.write(key, byteArrayOf(1)) }.exceptionOrNull()
        val read = runCatching { storage.read(key) }.exceptionOrNull()
        assertEquals(listOf(true, true), listOf(write is HostRejection, read is HostRejection))
        verify(exactly = 0) { encrypted.getDecryptedStringOrThrow(any()) }
    }

    @Test
    fun `corruption and failed removals propagate to the core`() = runTest {
        every { encrypted.getDecryptedStringOrThrow(any()) } throws IllegalStateException("invalid ciphertext")
        val decryption = runCatching { storage.read(key) }.exceptionOrNull()
        every { encrypted.getDecryptedStringOrThrow(any()) } returns "invalid"
        val encoding = runCatching { storage.read(key) }.exceptionOrNull()
        every { encrypted.removeKeyCommitted(any()) } throws IllegalStateException("disk full")
        val removal = runCatching { storage.clear(key) }.exceptionOrNull()
        assertEquals(listOf(true, true, true), listOf(decryption is HostRejection, encoding is HostRejection, removal is HostRejection))
    }
}
