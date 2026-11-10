package io.paritytech.polkadotapp.feature_chats_impl.data.hop.compaction

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.HopTicket
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.HopNodeUrlProvider
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.TerminalTransferException
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.CompactionExpansionRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatMessagePlacement
import io.paritytech.polkadotapp.feature_chats_impl.domain.compaction.CompactionExpansion
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.HopTransferRetryState
import io.paritytech.polkadotapp.feature_chats_impl.domain.sessions.IncomingChatMessageProcessor
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.eq
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.verify
import org.mockito.Mockito.verifyNoInteractions

class CompactionExpansionTransferQueueTest {
    private val nodeUrl = "wss://hop.example/node"
    private val commitId = "commit-1"
    private val contactAccountId: AccountId = ByteArray(32) { 9 }.toDataByteArray()
    private val batchBytes = byteArrayOf(0x0A, 0x0B)
    private val rawMessages = listOf(byteArrayOf(0x01), byteArrayOf(0x02))

    private val repository: CompactionExpansionRepository = mock(CompactionExpansionRepository::class.java)
    private val store: CompactionBatchStore = mock(CompactionBatchStore::class.java)
    private val nodeProvider: HopNodeUrlProvider = mock(HopNodeUrlProvider::class.java)
    private val processor: IncomingChatMessageProcessor = mock(IncomingChatMessageProcessor::class.java)

    private val queue = CompactionExpansionTransferQueue(
        repository = repository,
        compactionBatchStore = store,
        hopNodeUrlProvider = nodeProvider,
        incomingChatMessageProcessor = processor
    )

    @Test
    fun `process claims the batch then processes messages and marks the commit expanded`() = runBlocking<Unit> {
        withNodeAllowed(true)
        withClaimedBatch()
        whenever(store.decodeBatchMessages(batchBytes)).thenReturn(rawMessages)

        queue.process(expansion())

        verify(processor).processRaw(eq(contactAccountId), eq(rawMessages), eq(ChatMessagePlacement.SameAs(commitId)))
        verify(repository).markContentExpanded(commitId)
    }

    @Test
    fun `untrusted node is terminal and never claims`() = runBlocking<Unit> {
        withNodeAllowed(false)

        val result = runCatching { queue.process(expansion()) }

        assertTerminal(result)
        verifyNoInteractions(store)
        verifyNoInteractions(processor)
    }

    @Test
    fun `undecodable commit content is terminal and never claims`() = runBlocking<Unit> {
        val result = runCatching { queue.process(expansion(commit = null)) }

        assertTerminal(result)
        verifyNoInteractions(store)
        verifyNoInteractions(nodeProvider)
    }

    @Test
    fun `missing contact origin is terminal and never claims`() = runBlocking<Unit> {
        val result = runCatching { queue.process(expansion(contactAccountId = null)) }

        assertTerminal(result)
        verifyNoInteractions(store)
        verifyNoInteractions(nodeProvider)
    }

    @Test
    fun `markFailed surfaces the commit as unrecoverable`() = runBlocking<Unit> {
        queue.markFailed(expansion(), TerminalTransferException("gone"))

        verify(repository).markCompactionUnrecoverable(commitId)
    }

    private fun expansion(
        commit: ChatMessage.Content.CompactionCommit? = ChatMessage.Content.CompactionCommit(
            claimIdentifier = ByteArray(32).toDataByteArray(),
            claimTicket = HopTicket.fromRaw(ByteArray(32)),
            nodeUrl = nodeUrl
        ),
        contactAccountId: AccountId? = this.contactAccountId
    ) = CompactionExpansion(
        commitId = commitId,
        contactAccountId = contactAccountId,
        commit = commit,
        retryState = HopTransferRetryState.None
    )

    private suspend fun withNodeAllowed(allowed: Boolean) {
        whenever(nodeProvider.isAllowed(nodeUrl)).thenReturn(allowed)
    }

    private suspend fun withClaimedBatch() {
        whenever(store.claimBatch(any(), anyHopTicket(), any(), any())).thenAnswer { invocation ->
            @Suppress("UNCHECKED_CAST")
            val persist = invocation.arguments[3] as suspend (ByteArray) -> Unit
            runBlocking { persist(batchBytes) }
        }
    }

    private fun assertTerminal(result: Result<*>) {
        assertTrue(
            "expected TerminalTransferException but was ${result.exceptionOrNull()}",
            result.exceptionOrNull() is TerminalTransferException
        )
    }
}
