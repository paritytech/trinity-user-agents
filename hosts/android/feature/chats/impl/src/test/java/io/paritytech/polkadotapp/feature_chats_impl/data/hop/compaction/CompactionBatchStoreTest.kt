package io.paritytech.polkadotapp.feature_chats_impl.data.hop.compaction

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.encodeToByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.HopTicket
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.HopNodeUrlProvider
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.HopService
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.auth.HopSigner
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.HopChunkedPayload
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.HopPoolEntryPayload
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.VersionedHopPoolEntry
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.RetryableTransferException
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.TerminalTransferException
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.verifyNoInteractions

class CompactionBatchStoreTest {
    private val nodeUrl = "wss://hop.example/node"
    private val fallbackNodes = listOf(nodeUrl)
    private val ticket = HopTicket.fromRaw(ByteArray(32) { it.toByte() })
    private val identifier = ByteArray(32) { 1 }.toDataByteArray()
    private val rawMessages = listOf(byteArrayOf(0x01, 0x02), byteArrayOf(0x03))

    private val session: HopService.Session = mock(HopService.Session::class.java)
    private val hopService: HopService = mock(HopService::class.java)
    private val hopSigner: HopSigner = mock(HopSigner::class.java)
    private val hopNodeUrlProvider: HopNodeUrlProvider = mock(HopNodeUrlProvider::class.java)

    private var ackCalled = false

    private val store = CompactionBatchStore(
        hopService = hopService,
        hopSigner = hopSigner,
        hopNodeUrlProvider = hopNodeUrlProvider
    )

    @Before
    fun setUp() = runBlocking<Unit> {
        withHopSession()
        withFallbackNodes()
    }

    @Test
    fun `uploadBatch wraps the messages into an inline envelope and returns the entry hash`() = runBlocking<Unit> {
        val hash = ByteArray(32) { 7 }
        var submittedPlaintext: ByteArray? = null
        whenever(session.submitEntry(any(), anyHopTicket())).thenAnswer { invocation ->
            submittedPlaintext = invocation.arguments[0] as ByteArray
            hash
        }

        val result = store.uploadBatch(rawMessages, ticket, nodeUrl)

        assertArrayEquals(hash, result.value)
        assertBatchMessages(rawMessages, decodeInlineBatch(checkNotNull(submittedPlaintext)))
    }

    @Test
    fun `uploadBatch refuses a batch over the inline budget without submitting`() = runBlocking<Unit> {
        val oversized = listOf(ByteArray(HopService.INLINE_MAX_BYTES))

        val result = runCatching { store.uploadBatch(oversized, ticket, nodeUrl) }

        assertTrue(result.exceptionOrNull() is IllegalArgumentException)
        verifyNoInteractions(session)
    }

    @Test
    fun `claimBatch extracts the inline batch bytes and acks only after persist`() = runBlocking<Unit> {
        var persisted: ByteArray? = null
        var persistedBeforeAck = false
        whenever(session.fetchEntry(any(), anyHopTicket(), any())).thenReturn(
            HopService.FetchedEntry(inlineEnvelopeBytes()) {
                persistedBeforeAck = persisted != null
                ackCalled = true
            }
        )

        store.claimBatch(identifier, ticket, nodeUrl) { persisted = it }

        assertBatchMessages(rawMessages, store.decodeBatchMessages(checkNotNull(persisted)))
        assertTrue(ackCalled)
        assertTrue("expected persist to complete before ack", persistedBeforeAck)
    }

    @Test
    fun `claimBatch treats a chunked envelope as malformed and never persists`() = runBlocking<Unit> {
        var persisted: ByteArray? = null
        withFetchedEntry(chunkedEnvelopeBytes())

        val result = runCatching { store.claimBatch(identifier, ticket, nodeUrl) { persisted = it } }

        assertTerminal(result)
        assertNull(persisted)
        assertFalse(ackCalled)
    }

    @Test
    fun `claimBatch is retryable when the entry is not found in the pool or on any fallback node`() = runBlocking<Unit> {
        whenever(session.fetchEntry(any(), anyHopTicket(), any())).thenReturn(null)

        val result = runCatching { store.claimBatch(identifier, ticket, nodeUrl) {} }

        assertTrue(
            "expected RetryableTransferException but was ${result.exceptionOrNull()}",
            result.exceptionOrNull() is RetryableTransferException
        )
    }

    @Test
    fun `packBatches keeps a fitting set as a single chunk`() {
        val chunks = store.packBatches(rawMessages)

        assertEquals(1, chunks.size)
        assertBatchMessages(rawMessages, chunks.single())
    }

    @Test
    fun `packBatches packs a message filling the budget exactly into one chunk`() {
        val message = ByteArray(store.batchBudgetBytes - 5)

        val chunks = store.packBatches(listOf(message))

        assertEquals(1, chunks.size)
        assertEquals(store.batchBudgetBytes, BinaryScale.encodeToByteArray(chunks.single()).size)
    }

    @Test
    fun `packBatches splits messages that together exceed the budget preserving order`() {
        val first = ByteArray(1_000_000) { 1 }
        val second = ByteArray(1_000_000) { 2 }

        val chunks = store.packBatches(listOf(first, second))

        assertEquals(2, chunks.size)
        assertBatchMessages(listOf(first), chunks[0])
        assertBatchMessages(listOf(second), chunks[1])
        chunks.forEach { chunk ->
            assertTrue(BinaryScale.encodeToByteArray(chunk).size <= store.batchBudgetBytes)
        }
    }

    @Test
    fun `packBatches refuses a message that cannot fit even a dedicated chunk`() {
        val oversized = ByteArray(store.batchBudgetBytes - 4)

        val result = runCatching { store.packBatches(listOf(oversized)) }

        assertTrue(result.exceptionOrNull() is IllegalArgumentException)
    }

    @Test
    fun `claimBatch does not ack when persist fails`() = runBlocking<Unit> {
        withFetchedEntry(inlineEnvelopeBytes())

        val result = runCatching {
            store.claimBatch(identifier, ticket, nodeUrl) { error("persist failed") }
        }

        assertTrue(result.isFailure)
        assertFalse(ackCalled)
    }

    private suspend fun withHopSession() {
        whenever(hopService.withSession<Any?>(any(), any())).thenAnswer { invocation ->
            @Suppress("UNCHECKED_CAST")
            val block = invocation.arguments[1] as suspend HopService.Session.() -> Any?
            runBlocking { block(session) }
        }
    }

    private suspend fun withFallbackNodes() {
        whenever(hopNodeUrlProvider.allWithSenderPriority(nodeUrl)).thenReturn(fallbackNodes)
    }

    private suspend fun withFetchedEntry(bytes: ByteArray) {
        whenever(session.fetchEntry(any(), anyHopTicket(), any()))
            .thenReturn(HopService.FetchedEntry(bytes) { ackCalled = true })
    }

    private fun inlineEnvelopeBytes(): ByteArray {
        val envelope: VersionedHopPoolEntry = VersionedHopPoolEntry.V1(
            HopPoolEntryPayload.Inline(BinaryScale.encodeToByteArray(rawMessages))
        )
        return BinaryScale.encodeToByteArray(envelope)
    }

    private fun chunkedEnvelopeBytes(): ByteArray {
        val envelope: VersionedHopPoolEntry = VersionedHopPoolEntry.V1(
            HopPoolEntryPayload.Chunked(HopChunkedPayload(totalSize = 10uL, chunks = listOf(ByteArray(32))))
        )
        return BinaryScale.encodeToByteArray(envelope)
    }

    private fun decodeInlineBatch(plaintext: ByteArray): List<ByteArray> {
        val envelope = BinaryScale.decodeFromByteArray<VersionedHopPoolEntry>(plaintext)
        val inline = (envelope as VersionedHopPoolEntry.V1).payload as HopPoolEntryPayload.Inline
        return BinaryScale.decodeFromByteArray<List<ByteArray>>(inline.bytes)
    }

    private fun assertBatchMessages(expected: List<ByteArray>, actual: List<ByteArray>) {
        assertEquals(expected.size, actual.size)
        expected.zip(actual).forEach { (expectedMessage, actualMessage) ->
            assertArrayEquals(expectedMessage, actualMessage)
        }
    }

    private fun assertTerminal(result: Result<*>) {
        assertTrue(
            "expected TerminalTransferException but was ${result.exceptionOrNull()}",
            result.exceptionOrNull() is TerminalTransferException
        )
    }
}
