package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.storage.preferences.Editor
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import io.paritytech.polkadotapp.common.utils.X25519KeyGenerator
import io.paritytech.polkadotapp.feature_statement_store_api.domain.OurDeviceKeypairProvider
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.SecretCoreStorageKey
import uniffi.truapi.secretCoreStorageKeyIdentifier
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class TrUAPISecretStorageTest {
    private val preferences = mockk<EncryptedPreferences>()
    private val backing = mockk<Preferences>()
    private val editor = mockk<Editor>()
    private val deviceKeys = mockk<OurDeviceKeypairProvider>()
    private val gate = TrUAPIStorageGate(backing)
    private val storage = TrUAPISecretStorage(preferences, gate, deviceKeys)
    private val key = SecretCoreStorageKey.AuthSession
    private val identifier = secretCoreStorageKeyIdentifier(key)

    init {
        every { backing.edit() } returns editor
        every { editor.commit() } returns true
    }

    @Test
    fun `only missing values return null`() = runTest {
        every { preferences.getDecryptedStringOrThrow(identifier) } returns null
        assertNull(storage.read(key))
        for (corrupt in listOf("", "untagged", "v1:zz", "v1:0")) {
            every { preferences.getDecryptedStringOrThrow(identifier) } returns corrupt
            assertNotNull(runCatching { storage.read(key) }.exceptionOrNull())
        }
        val failure = IllegalStateException("Keystore unavailable")
        every { preferences.getDecryptedStringOrThrow(identifier) } throws failure
        assertSame(failure, runCatching { storage.read(key) }.exceptionOrNull())
    }

    @Test
    fun `native allowances and paired sessions survive adapter recreation in separate namespaces`() = runTest {
        val values = mutableMapOf<String, String>()
        every { preferences.putEncryptedStringCommitted(any(), any()) } answers { values[firstArg()] = secondArg() }
        every { preferences.getDecryptedStringOrThrow(any()) } answers { values[firstArg()] }
        val first = SecretCoreStorageKey.AllowanceKeys("first")
        val second = SecretCoreStorageKey.AllowanceKeys("second")
        val native = SecretCoreStorageKey.NativeAllowanceKeys
        storage.write(first, byteArrayOf(1))
        storage.write(second, byteArrayOf(2))
        EncryptedHostCoreStorage(preferences, gate).write(byteArrayOf(0), byteArrayOf(3))
        storage.write(native, byteArrayOf(4))

        val recreatedGate = TrUAPIStorageGate(backing)
        val recreatedStorage = TrUAPISecretStorage(preferences, recreatedGate, deviceKeys)
        assertEquals(4, values.size)
        assertArrayEquals(byteArrayOf(1), recreatedStorage.read(first))
        assertArrayEquals(byteArrayOf(2), recreatedStorage.read(second))
        assertArrayEquals(byteArrayOf(3), EncryptedHostCoreStorage(preferences, recreatedGate).read(byteArrayOf(0)))
        assertArrayEquals(byteArrayOf(4), recreatedStorage.read(native))
    }

    @Test
    fun `failed writes and deletes cannot be acknowledged from memory before a successful flush`() = runTest {
        for (deleting in listOf(false, true)) {
            var memory: String? = "v1:01"
            var durable = memory
            var canFlush = true
            every { editor.commit() } answers {
                if (canFlush) durable = memory
                canFlush
            }
            every { preferences.getDecryptedStringOrThrow(identifier) } answers { memory }
            val failure = IllegalStateException("commit failed after updating memory")
            every { preferences.putEncryptedStringCommitted(identifier, any()) } answers {
                memory = secondArg()
                canFlush = false
                throw failure
            }
            every { preferences.removeKeyCommitted(identifier) } answers {
                memory = null
                canFlush = false
                throw failure
            }
            val mutation = runCatching {
                if (deleting) storage.clear(key) else storage.write(key, byteArrayOf(2))
            }
            assertSame(failure, mutation.exceptionOrNull())
            assertNotNull(runCatching { storage.read(key) }.exceptionOrNull())
            assertEquals("v1:01", durable)
            canFlush = true
            assertArrayEquals(if (deleting) null else byteArrayOf(2), storage.read(key))
            assertEquals(if (deleting) null else "v1:02", durable)
        }
    }

    @Test
    fun `mixed cleanup waits for a cancelled write already inside storage`() = runTest {
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        val values = ConcurrentHashMap<String, String>()
        every { preferences.putEncryptedStringCommitted(identifier, any()) } answers {
            entered.countDown()
            check(release.await(5, TimeUnit.SECONDS))
            values[identifier] = secondArg()
        }
        every { preferences.removeKeyCommitted(any()) } answers {
            values.remove(firstArg<String>())
            Unit
        }
        every { preferences.getDecryptedStringOrThrow(any()) } answers { values[firstArg()] }
        val writer = async(Dispatchers.IO) { storage.write(key, byteArrayOf(4)) }
        assertTrue(entered.await(5, TimeUnit.SECONDS))
        writer.cancel()
        val cleanup = async(start = CoroutineStart.UNDISPATCHED) {
            EncryptedHostCoreStorage(preferences, gate).clear(byteArrayOf(2))
            storage.clear(key)
        }
        try {
            assertFalse(cleanup.isCompleted)
        } finally {
            release.countDown()
        }
        cleanup.await()
        writer.join()
        assertNull(storage.read(key))
        verify(exactly = 1) { preferences.removeKeyCommitted("truapi/core/02") }
    }

    @Test
    fun `a cancelled waiter cannot write after cleanup`() = runTest {
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        every { preferences.getDecryptedStringOrThrow(identifier) } answers {
            entered.countDown()
            check(release.await(5, TimeUnit.SECONDS))
            null
        }
        every { preferences.removeKeyCommitted(identifier) } returns Unit
        val read = async(Dispatchers.IO) { storage.read(key) }
        assertTrue(entered.await(5, TimeUnit.SECONDS))
        val writer = async(start = CoroutineStart.UNDISPATCHED) { storage.write(key, byteArrayOf(5)) }
        writer.cancel()
        val cleanup = async(start = CoroutineStart.UNDISPATCHED) { storage.clear(key) }
        release.countDown()
        read.await()
        cleanup.await()
        writer.join()
        verify(exactly = 0) { preferences.putEncryptedStringCommitted(any(), any()) }
    }

    @Test
    fun `shared device identity comes only from its existing provider`() = runTest {
        val device = X25519KeyGenerator().generateRandomKeypair()
        coEvery { deviceKeys.get() } returns device
        assertArrayEquals(device.privateKey.bytes.value, storage.read(SecretCoreStorageKey.DeviceEncryptionKey))
        assertNotNull(runCatching { storage.write(SecretCoreStorageKey.DeviceEncryptionKey, byteArrayOf(1)) }.exceptionOrNull())
        assertNotNull(runCatching { storage.clear(SecretCoreStorageKey.DeviceEncryptionKey) }.exceptionOrNull())
        coVerify(exactly = 1) { deviceKeys.get() }
        verify(exactly = 0) { preferences.getDecryptedStringOrThrow(any()) }
        verify(exactly = 0) { preferences.putEncryptedStringCommitted(any(), any()) }
        verify(exactly = 0) { preferences.removeKeyCommitted(any()) }
    }
}
