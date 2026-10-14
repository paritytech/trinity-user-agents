package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.paritytech.polkadotapp.database.dao.ChatRequestDao
import io.paritytech.polkadotapp.database.dao.ChatRequestSyncStateDao
import io.paritytech.polkadotapp.database.model.ChatRequestLocal
import io.paritytech.polkadotapp.database.model.ChatRequestSyncStateLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toDomain
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toLocal
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

interface ChatRequestRepository {
    suspend fun save(request: ChatRequest)

    suspend fun getById(id: String): ChatRequest?

    fun subscribeById(id: String): Flow<ChatRequest?>

    suspend fun updateStatus(id: String, status: ChatRequest.Status)

    suspend fun delete(id: String)

    suspend fun getLastSyncedDay(metaAccountId: Long): Long?

    suspend fun updateLastSyncedDay(metaAccountId: Long, day: Long)
}

suspend fun ChatRequestRepository.getByIdOrThrow(id: String): ChatRequest {
    return requireNotNull(getById(id)) {
        "No request found for id $id"
    }
}

class RealChatRequestRepository @Inject constructor(
    private val chatRequestDao: ChatRequestDao,
    private val syncStateDao: ChatRequestSyncStateDao
) : ChatRequestRepository {
    override suspend fun save(request: ChatRequest) {
        chatRequestDao.upsert(request.toLocal())
    }

    override suspend fun getById(id: String): ChatRequest? {
        return chatRequestDao.getById(id)?.toDomain()
    }

    override fun subscribeById(id: String): Flow<ChatRequest?> {
        return chatRequestDao.subscribeById(id).map { it?.toDomain() }
    }

    override suspend fun updateStatus(id: String, status: ChatRequest.Status) {
        chatRequestDao.updateStatus(id, status.toLocalStatus())
    }

    override suspend fun delete(id: String) {
        chatRequestDao.delete(id)
    }

    override suspend fun getLastSyncedDay(metaAccountId: Long): Long? {
        return syncStateDao.get(metaAccountId)?.lastSyncedDay
    }

    override suspend fun updateLastSyncedDay(metaAccountId: Long, day: Long) {
        syncStateDao.upsert(
            ChatRequestSyncStateLocal(
                metaAccountId = metaAccountId,
                lastSyncedDay = day,
            )
        )
    }

    private fun ChatRequest.Status.toLocalStatus(): ChatRequestLocal.Status {
        return when (this) {
            ChatRequest.Status.PENDING -> ChatRequestLocal.Status.PENDING
            ChatRequest.Status.ACCEPTED -> ChatRequestLocal.Status.ACCEPTED
            ChatRequest.Status.DECLINED -> ChatRequestLocal.Status.DECLINED
        }
    }
}
