package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.database.dao.ChatMessageCompactionDao
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_impl.domain.compaction.CompactionExpansion
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.ChatMessageContentLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toDomain
import kotlinx.serialization.encodeToByteArray
import javax.inject.Inject
import kotlin.time.Instant

interface CompactionExpansionRepository {
    suspend fun getNextPending(): CompactionExpansion?

    suspend fun scheduleRetry(
        commitId: ChatMessageId,
        attemptCount: Int,
        firstFailureAt: Instant,
        nextAttemptAt: Instant
    )

    suspend fun getSoonestNextAttemptAt(): Instant?

    suspend fun markContentExpanded(commitId: ChatMessageId)

    suspend fun markCompactionUnrecoverable(commitId: ChatMessageId)
}

class RealCompactionExpansionRepository @Inject constructor(
    private val chatMessageCompactionDao: ChatMessageCompactionDao
) : CompactionExpansionRepository {
    override suspend fun getNextPending(): CompactionExpansion? {
        val row = chatMessageCompactionDao.getNextPendingExpansion(System.currentTimeMillis()) ?: return null

        val content = row.commit.toDomain(AlwaysFailCustomContentDecoder()).content

        return CompactionExpansion(
            commitId = row.expansion.commitId,
            contactAccountId = row.commit.origin.key?.toDataByteArray(),
            commit = content as? ChatMessage.Content.CompactionCommit,
            retryState = row.expansion.retryState.toDomain()
        )
    }

    override suspend fun scheduleRetry(
        commitId: ChatMessageId,
        attemptCount: Int,
        firstFailureAt: Instant,
        nextAttemptAt: Instant
    ) {
        chatMessageCompactionDao.scheduleExpansionRetry(
            commitId = commitId,
            attemptCount = attemptCount,
            firstFailureAt = firstFailureAt.toEpochMilliseconds(),
            nextAttemptAt = nextAttemptAt.toEpochMilliseconds()
        )
    }

    override suspend fun getSoonestNextAttemptAt(): Instant? {
        return chatMessageCompactionDao.getSoonestExpansionNextAttemptAt()?.let(Instant::fromEpochMilliseconds)
    }

    override suspend fun markContentExpanded(commitId: ChatMessageId) {
        chatMessageCompactionDao.clearPendingExpansion(commitId)
    }

    override suspend fun markCompactionUnrecoverable(commitId: ChatMessageId) {
        val content = BinaryScale.encodeToByteArray<ChatMessageContentLocal>(ChatMessageContentLocal.CompactionUnavailable)
        chatMessageCompactionDao.markCompactionUnrecoverable(commitId, content, updatedAt = System.currentTimeMillis())
    }
}
