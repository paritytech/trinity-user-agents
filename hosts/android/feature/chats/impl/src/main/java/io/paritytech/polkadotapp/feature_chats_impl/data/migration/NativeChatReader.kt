package io.paritytech.polkadotapp.feature_chats_impl.data.migration

import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.requireX25519PublicKey
import io.paritytech.polkadotapp.database.dao.ChatMessageProcessingDao
import io.paritytech.polkadotapp.database.dao.ChatRequestDao
import io.paritytech.polkadotapp.database.dao.ContactDao
import io.paritytech.polkadotapp.database.dao.ContactDeviceDao
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactAccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactDevice
import io.paritytech.polkadotapp.feature_chats_impl.data.model.toDomain
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toDomain
import javax.inject.Inject

/**
 * Reads what the native person chat left in the database, for the one-way move of an install to the chat product
 * (the history export and the payments the host takes over). Nothing writes these tables any more.
 */
class NativeChatReader @Inject constructor(
    private val contactDao: ContactDao,
    private val contactDeviceDao: ContactDeviceDao,
    private val chatRequestDao: ChatRequestDao,
    private val processingDao: ChatMessageProcessingDao,
) {
    suspend fun contacts(): List<Contact> = contactDao.getAll().map { it.toDomain() }

    suspend fun devices(contactAccountId: ContactAccountId): List<ContactDevice> =
        contactDeviceDao.getByContact(contactAccountId.value).map {
            ContactDevice(
                contactAccountId = it.contactAccountId.intoAccountId(),
                statementAccountId = it.statementAccountId.intoAccountId(),
                encryptionPublicKey = it.encryptionPublicKey.requireX25519PublicKey(),
            )
        }

    suspend fun request(id: String): ChatRequest? = chatRequestDao.getById(id)?.toDomain()

    /** The ids of the stored messages that the chat extension [extensionId] has not finished with. */
    suspend fun unprocessedMessageIds(extensionId: String): Set<ChatMessageId> =
        processingDao.getUnprocessedMessages(extensionId).mapTo(mutableSetOf()) { it.id }
}
