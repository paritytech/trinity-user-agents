package io.paritytech.polkadotapp.feature_chats_impl.data.resources

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.blake2b256
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertNull
import org.junit.Test
import java.security.MessageDigest

/** The vector comes from the `chacha20poly1305` crate of the core, the same one that the iOS app checks. */
class ChatSealedFileTest {
    private val key = ByteArray(32) { it.toByte() }
    private val nonce = ByteArray(24) { (100 + it).toByte() }
    private val file = "a sealed attachment of the chat".encodeToByteArray()
    private val ciphertext = "1d538ad59e42ece6a751e94bc70cd5a43ed526518b4aeac4e43044c4433f0cfa4dc9a5e93f656e666e4a97c2950479".fromHex()

    @Test
    fun `HChaCha20 gives the subkey of the draft vector`() {
        val subkey = HChaCha20.subkey(ByteArray(32) { it.toByte() }, "000000090000004a0000000031415927".fromHex())

        assertArrayEquals("82413b4227b27bfed30e42508a877d73a0f9e4d58a74a853c12ec41326d3ecdc".fromHex(), subkey)
    }

    @Test
    fun `a sealed file opens from its parts in order`() = runBlocking {
        val parts = listOf(ciphertext.copyOfRange(0, 20), ciphertext.copyOfRange(20, ciphertext.size))
        val stored = parts.associateBy { it.blake2b256().toList() }

        val opened = ChatSealedFile.read(resource(parts.map { it.blake2b256() })) { stored[it.toList()] }

        assertArrayEquals(file, opened)
    }

    @Test
    fun `a changed part or another key opens nothing`() = runBlocking {
        val resource = resource(listOf(ciphertext.blake2b256()))
        val forged = ciphertext.copyOf().also { it[0] = (it[0].toInt() xor 1).toByte() }

        assertNull(ChatSealedFile.read(resource) { forged })
        assertNull(ChatSealedFile.open(resource.copy(key = ByteArray(32) { 7 }.toDataByteArray()), ciphertext))
    }

    private fun resource(parts: List<ByteArray>) = ChatResourceModel.Sealed(
        parts = parts.map { it.toDataByteArray() },
        key = key.toDataByteArray(),
        nonce = nonce.toDataByteArray(),
        size = ciphertext.size.toLong(),
        digest = MessageDigest.getInstance("SHA-512").digest(ciphertext).toDataByteArray(),
    )
}
