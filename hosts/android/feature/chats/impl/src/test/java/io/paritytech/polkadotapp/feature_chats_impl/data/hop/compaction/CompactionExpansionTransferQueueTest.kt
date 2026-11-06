package io.paritytech.polkadotapp.feature_chats_impl.data.hop.compaction

import io.mockk.Called
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
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
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Test

class CompactionExpansionTransferQueueTest {
    private val nodeUrl = "wss://hop.example/node"
    private val commitId = "commit-1"
    private val contactAccountId: AccountId = ByteArray(32) { 9 }.toDataByteArray()
    private val batchBytes = byteArrayOf(0x0A, 0x0B)
    private val rawMessages = listOf(byteArrayOf(0x01), byteArrayOf(0x02))

    private val repository: CompactionExpansionRepository = mockk(relaxUnitFun = true)
    private val store: CompactionBatchStore = mockk()
    private val nodeProvider: HopNodeUrlProvider = mockk()
    private val processor: IncomingChatMessageProcessor = mockk(relaxUnitFun = true)

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
        every { store.decodeBatchMessages(batchBytes) } returns rawMessages

        queue.process(expansion())

        coVerify(exactly = 1) { processor.processRaw(contactAccountId, rawMessages, ChatMessagePlacement.SameAs(commitId)) }
        coVerify(exactly = 1) { repository.markContentExpanded(commitId) }
    }

    @Test
    fun `untrusted node is terminal and never claims`() = runBlocking<Unit> {
        withNodeAllowed(false)

        val result = runCatching { queue.process(expansion()) }

        assertTerminal(result)
        verify { store wasNot Called }
        verify { processor wasNot Called }
    }

    @Test
    fun `undecodable commit content is terminal and never claims`() = runBlocking<Unit> {
        val result = runCatching { queue.process(expansion(commit = null)) }

        assertTerminal(result)
        verify { store wasNot Called }
        verify { nodeProvider wasNot Called }
    }

    @Test
    fun `missing contact origin is terminal and never claims`() = runBlocking<Unit> {
        val result = runCatching { queue.process(expansion(contactAccountId = null)) }

        assertTerminal(result)
        verify { store wasNot Called }
        verify { nodeProvider wasNot Called }
    }

    @Test
    fun `markFailed surfaces the commit as unrecoverable`() = runBlocking<Unit> {
        queue.markFailed(expansion(), TerminalTransferException("gone"))

        coVerify(exactly = 1) { repository.markCompactionUnrecoverable(commitId) }
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
        coEvery { nodeProvider.isAllowed(nodeUrl) } returns allowed
    }

    private suspend fun withClaimedBatch() {
        coEvery { store.claimBatch(any(), any(), any(), any()) } coAnswers {
            val persist = arg<suspend (ByteArray) -> Unit>(3)
            persist(batchBytes)
        }
    }

    private fun assertTerminal(result: Result<*>) {
        assertTrue(
            "expected TerminalTransferException but was ${result.exceptionOrNull()}",
            result.exceptionOrNull() is TerminalTransferException
        )
    }
}
