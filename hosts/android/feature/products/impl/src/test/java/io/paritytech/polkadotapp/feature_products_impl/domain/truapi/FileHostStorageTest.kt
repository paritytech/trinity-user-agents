package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptionUtil
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import uniffi.truapi.HostLocalStorageReadException
import javax.crypto.spec.SecretKeySpec

class FileHostStorageTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val prefs = mutableMapOf<String, String>()
    private val legacy = mockk<EncryptedPreferences> {
        every { getDecryptedString(any()) } answers { prefs[firstArg()] }
        every { putEncryptedString(any(), any()) } answers { prefs[firstArg()] = secondArg() }
        every { removeKey(any()) } answers { prefs.remove(firstArg<String>()) }
    }
    private val cipher = ProductStorageCipher(
        mockk<EncryptionUtil> { every { getPrerenceAesKey() } returns SecretKeySpec(ByteArray(32) { 7 }, "AES") }
    )

    private fun files(budget: Long = 1 shl 20) = ProductStorageFiles(folder.root, cipher, legacy, budget)

    @Test
    fun `a value survives a restart, sealed on disk`() = runTest {
        files().storage("chat.dot").write("pc/v1/meta", "hello".encodeToByteArray())

        assertArrayEquals("hello".encodeToByteArray(), files().storage("chat.dot").read("pc/v1/meta"))
        val onDisk = folder.root.walkTopDown().filter { it.isFile }.single().readBytes()
        assertFalse(onDisk.decodeToString().contains("hello"))
        assertNull(files().storage("other.dot").read("pc/v1/meta"))
    }

    @Test
    fun `a value an earlier build kept in encrypted preferences moves to a file when read`() = runTest {
        prefs["truapi/product/chat.dot/pc/v1/meta"] = "v1:0102"

        assertArrayEquals(byteArrayOf(1, 2), files().storage("chat.dot").read("pc/v1/meta"))
        assertEquals(emptyMap<String, String>(), prefs)
        assertArrayEquals(byteArrayOf(1, 2), files().storage("chat.dot").read("pc/v1/meta"))
    }

    @Test
    fun `a product past its budget gets Full, and a clear makes room again`() = runTest {
        // 100 bytes sealed take 128: a 12-byte nonce and a 16-byte tag.
        val storage = files(budget = 300).storage("chat.dot")
        storage.write("a", ByteArray(100))
        storage.write("b", ByteArray(100))
        // Rewriting a key counts its new size only.
        storage.write("b", ByteArray(100))

        assertThrows(HostLocalStorageReadException.Full::class.java) { runBlocking { storage.write("c", ByteArray(100)) } }
        storage.clear("a")
        storage.write("c", ByteArray(100))
        assertNull(storage.read("a"))
    }

    @Test
    fun `a file this install did not seal reads as missing`() = runTest {
        val storage = files().storage("chat.dot")
        storage.write("k", byteArrayOf(1))
        folder.root.walkTopDown().filter { it.isFile }.single().writeBytes(ByteArray(40))

        assertNull(storage.read("k"))
    }
}
