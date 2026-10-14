package io.paritytech.polkadotapp.feature_chats_impl.data.hop.compaction

import io.paritytech.polkadotapp.feature_chats_impl.data.hop.HopNodeUrlProvider
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.RetryDecision
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.TerminalTransferException
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.TransferQueue
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.CompactionExpansionRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatMessagePlacement
import io.paritytech.polkadotapp.feature_chats_impl.domain.compaction.CompactionExpansion
import io.paritytech.polkadotapp.feature_chats_impl.domain.sessions.IncomingChatMessageProcessor
import javax.inject.Inject
import kotlin.time.Instant

class CompactionExpansionTransferQueue @Inject constructor(
    private val repository: CompactionExpansionRepository,
    private val compactionBatchStore: CompactionBatchStore,
    private val hopNodeUrlProvider: HopNodeUrlProvider,
    private val incomingChatMessageProcessor: IncomingChatMessageProcessor
) : TransferQueue<CompactionExpansion> {
    override suspend fun nextPending(): CompactionExpansion? = repository.getNextPending()

    override suspend fun markInProgress(item: CompactionExpansion) = Unit

    override suspend fun process(item: CompactionExpansion) {
        val commit = item.commit
        val contactAccountId = item.contactAccountId

        if (commit == null || contactAccountId == null) {
            throw TerminalTransferException(
                "Compaction commit ${item.commitId} carries no decodable CompactionCommit content or contact origin"
            )
        }

        if (!hopNodeUrlProvider.isAllowed(commit.nodeUrl)) {
            throw TerminalTransferException(
                "Compaction commit ${item.commitId} references untrusted node ${commit.nodeUrl}"
            )
        }

        compactionBatchStore.claimBatch(
            identifier = commit.claimIdentifier,
            ticket = commit.claimTicket,
            nodeUrl = commit.nodeUrl
        ) { batchBytes ->
            val rawMessages = compactionBatchStore.decodeBatchMessages(batchBytes)
            incomingChatMessageProcessor.processRaw(contactAccountId, rawMessages, ChatMessagePlacement.SameAs(item.commitId))
            repository.markContentExpanded(item.commitId)
        }
    }

    override suspend fun markDone(item: CompactionExpansion) = Unit

    override suspend fun markFailed(item: CompactionExpansion, error: Throwable) =
        repository.markCompactionUnrecoverable(item.commitId)

    override suspend fun scheduleRetry(item: CompactionExpansion, reschedule: RetryDecision.Reschedule) =
        repository.scheduleRetry(item.commitId, reschedule.attemptCount, reschedule.firstFailureAt, reschedule.nextAttemptAt)

    override suspend fun soonestNextAttemptAt(): Instant? = repository.getSoonestNextAttemptAt()
}
