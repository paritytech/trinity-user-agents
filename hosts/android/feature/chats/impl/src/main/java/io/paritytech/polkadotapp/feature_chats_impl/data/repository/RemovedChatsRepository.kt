package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.database.dao.RemovedChatDao
import io.paritytech.polkadotapp.database.model.RemovedChatLocal
import javax.inject.Inject
import kotlin.time.Instant

interface RemovedChatsRepository {
    suspend fun recordRemoval(accountId: AccountId, removedAt: Long)

    suspend fun getRemovedAfter(after: Instant): List<AccountId>
}

class RealRemovedChatsRepository @Inject constructor(
    private val removedChatDao: RemovedChatDao,
) : RemovedChatsRepository {
    override suspend fun recordRemoval(accountId: AccountId, removedAt: Long) {
        removedChatDao.upsert(RemovedChatLocal(accountId = accountId.value, removedAt = removedAt))
    }

    override suspend fun getRemovedAfter(after: Instant): List<AccountId> {
        return removedChatDao.getRemovedAfter(after.toEpochMilliseconds()).map { it.accountId.toDataByteArray() }
    }
}
