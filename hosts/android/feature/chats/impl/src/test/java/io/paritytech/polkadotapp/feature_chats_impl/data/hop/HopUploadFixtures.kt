package io.paritytech.polkadotapp.feature_chats_impl.data.hop

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import org.mockito.Mockito

const val NODE_A = "wss://hop-a.example"
const val NODE_B = "wss://hop-b.example"
const val NODE_C = "wss://hop-c.example"

const val THREE_CHUNK_FILE_SIZE = 2 * HopService.CHUNK_SIZE_BYTES + 1234

// 251 is prime, so no chunk boundary lines up with the pattern and a slice from the wrong offset differs.
private const val PATTERN_MODULUS = 251

fun patternBytes(size: Int): ByteArray = ByteArray(size) { (it % PATTERN_MODULUS).toByte() }

fun ByteArray.chunkSlices(): List<ByteArray> =
    (indices step HopService.CHUNK_SIZE_BYTES).map { start ->
        copyOfRange(start, minOf(start + HopService.CHUNK_SIZE_BYTES, size))
    }

fun anyChatId(): ChatId {
    Mockito.argThat<Any?> { true }
    return ChatId.fromRawValue(ByteArray(0))
}
