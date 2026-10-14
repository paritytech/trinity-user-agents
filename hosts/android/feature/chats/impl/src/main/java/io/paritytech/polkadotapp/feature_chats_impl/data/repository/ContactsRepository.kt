package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.paritytech.polkadotapp.common.data.os.OperatingSystem
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.mapList
import io.paritytech.polkadotapp.common.utils.mapToUnit
import io.paritytech.polkadotapp.database.dao.ContactDao
import io.paritytech.polkadotapp.feature_chats_api.domain.BlockedContactsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatPushId
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatPushToken
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactWithChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactWithRequestTimestamp
import io.paritytech.polkadotapp.feature_chats_impl.data.model.toDomain
import io.paritytech.polkadotapp.feature_chats_impl.data.model.toLocal
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject
import kotlin.time.Instant

interface ContactsRepository {
    suspend fun getContacts(): List<Contact>
    suspend fun getContact(accountId: AccountId): Contact?
    suspend fun getContactByPushId(pushId: ChatPushId): Contact?
    suspend fun getAddedAfter(after: Instant): List<Contact>

    /** Established contacts (`establishedAt != null && > after`). Used for device-sync. */
    suspend fun getEstablishedAfter(after: Instant): List<Contact>

    fun subscribeContacts(): Flow<List<Contact>>
    fun subscribeContact(accountId: AccountId): Flow<Contact?>

    fun observeContactsChanged(): Flow<Unit>

    suspend fun saveContact(contact: Contact)
    suspend fun updateContactUsername(accountId: AccountId, username: String?)
    suspend fun updateContactPushToken(accountId: AccountId, token: ChatPushToken, operatingSystem: OperatingSystem)
    suspend fun updateContactVoipPushToken(accountId: AccountId, token: ChatPushToken)
    suspend fun updateLastSharedPushTokenFor(accounts: List<AccountId>, newPushToken: String)
    suspend fun updatePushId(accountId: AccountId, newPushId: ChatPushId)
    suspend fun deleteContact(accountId: AccountId)
    suspend fun setPeerLeft(accountId: AccountId, isLeft: Boolean)
    suspend fun setBlocked(accountId: AccountId, isBlocked: Boolean)
    fun subscribeBlockedContacts(): Flow<List<Contact>>
    suspend fun updateChatRequestId(accountId: AccountId, chatRequestId: String?)

    suspend fun markChatRequestAccepted(accountId: AccountId, establishedAt: Instant)

    fun subscribeContactsWithPendingIncomingRequests(): Flow<List<ContactWithRequestTimestamp>>

    fun subscribePendingIncomingRequestsCount(): Flow<Int>

    fun subscribeContactWithChatRequest(accountId: AccountId): Flow<ContactWithChatRequest?>

    fun subscribeContactsWithChatRequests(): Flow<List<ContactWithChatRequest>>

    suspend fun getContactsWithChatRequests(metaId: Long): List<ContactWithChatRequest>

    fun subscribePendingFanOutContacts(): Flow<List<Contact>>

    suspend fun markDevicesFannedOut(accountId: AccountId)
}

suspend fun ContactsRepository.getContactOrThrow(accountId: AccountId): Contact {
    return getContact(accountId) ?: throw IllegalStateException("Contact with accountId=$accountId not found")
}

suspend fun ContactsRepository.getContactResult(accountId: AccountId): Result<Contact> {
    return getContact(accountId)?.let { Result.success(it) }
        ?: Result.failure(IllegalStateException("Contact with accountId=$accountId not found"))
}

fun ContactsRepository.subscribeContactsWithChatRequestsByContactId(): Flow<Map<AccountId, ContactWithChatRequest>> {
    return subscribeContactsWithChatRequests().map { contacts ->
        contacts.associateBy { it.contact.accountId }
    }
}

class RealContactsRepository @Inject constructor(
    private val dao: ContactDao,
    private val chatSearchRecentsRepository: ChatSearchRecentsRepository,
) : ContactsRepository, BlockedContactsRepository {
    override suspend fun getContacts(): List<Contact> {
        return dao.getAll().map { it.toDomain() }
    }

    override suspend fun getContact(accountId: AccountId): Contact? {
        return dao.getByAccountId(accountId.value)?.toDomain()
    }

    override suspend fun getContactByPushId(pushId: ChatPushId): Contact? {
        return dao.getByPushId(pushId.value)?.toDomain()
    }

    override suspend fun getAddedAfter(after: Instant): List<Contact> {
        return dao.getAddedAfter(after.toEpochMilliseconds()).map { it.toDomain() }
    }

    override suspend fun getEstablishedAfter(after: Instant): List<Contact> {
        return dao.getEstablishedAfter(after.toEpochMilliseconds()).map { it.toDomain() }
    }

    override fun subscribeContacts(): Flow<List<Contact>> {
        return dao.subscribeAll()
            .map { contacts -> contacts.map { it.toDomain() } }
    }

    override fun observeContactsChanged(): Flow<Unit> = dao.observeContactCount().mapToUnit()

    override suspend fun saveContact(contact: Contact) {
        dao.upsert(contact.toLocal())
    }

    override suspend fun updateContactUsername(accountId: AccountId, username: String?) {
        dao.updateUsername(accountId.value, username)
    }

    override suspend fun updateContactPushToken(
        accountId: AccountId,
        token: ChatPushToken,
        operatingSystem: OperatingSystem
    ) {
        dao.updatePushTokenAndOperatingSystem(
            accountId = accountId.value,
            pushToken = token.value,
            operatingSystem = operatingSystem.toLocal()
        )
    }

    override suspend fun updateContactVoipPushToken(accountId: AccountId, token: ChatPushToken) {
        dao.updateVoipPushToken(accountId.value, token.value)
    }

    override suspend fun updateLastSharedPushTokenFor(
        accounts: List<AccountId>,
        newPushToken: String
    ) {
        dao.withTransaction {
            accounts.forEach { accountId ->
                dao.updateLastSharedPushToken(accountId.value, newPushToken)
            }
        }
    }

    override suspend fun updatePushId(
        accountId: AccountId,
        newPushId: ChatPushId
    ) {
        dao.updatePushId(accountId.value, newPushId.value)
    }

    override suspend fun deleteContact(accountId: AccountId) {
        dao.delete(accountId.value)
        chatSearchRecentsRepository.removeRecent(ChatId.fromContact(accountId))
    }

    override suspend fun setPeerLeft(accountId: AccountId, isLeft: Boolean) {
        dao.updatePeerLeft(accountId.value, isLeft)
    }

    override suspend fun setBlocked(accountId: AccountId, isBlocked: Boolean) {
        dao.updateIsBlocked(accountId.value, isBlocked)
    }

    override fun subscribeBlockedContacts(): Flow<List<Contact>> {
        return dao.subscribeBlocked()
            .mapList { it.toDomain() }
    }

    override fun subscribeHasBlockedContacts(): Flow<Boolean> {
        return dao.subscribeHasBlockedContacts()
    }

    override fun subscribeContact(accountId: AccountId): Flow<Contact?> {
        return dao.subscribeByAccountId(accountId.value)
            .map { it?.toDomain() }
    }

    override suspend fun updateChatRequestId(accountId: AccountId, chatRequestId: String?) {
        dao.updateChatRequestId(accountId.value, chatRequestId)
    }

    override suspend fun markChatRequestAccepted(accountId: AccountId, establishedAt: Instant) {
        dao.markChatRequestAccepted(accountId.value, establishedAt.toEpochMilliseconds())
    }

    override fun subscribeContactsWithPendingIncomingRequests(): Flow<List<ContactWithRequestTimestamp>> {
        return dao.subscribeContactsWithPendingIncomingRequests()
            .map { contactsWithTimestamp -> contactsWithTimestamp.map { it.toDomain() } }
    }

    override fun subscribePendingIncomingRequestsCount(): Flow<Int> {
        return dao.subscribePendingIncomingRequestsCount()
    }

    override fun subscribeContactWithChatRequest(accountId: AccountId): Flow<ContactWithChatRequest?> {
        return dao.subscribeContactWithChatRequest(accountId.value)
            .map { it?.toDomain() }
    }

    override fun subscribeContactsWithChatRequests(): Flow<List<ContactWithChatRequest>> {
        return dao.subscribeContactsWithChatRequests()
            .mapList { it.toDomain() }
    }

    override suspend fun getContactsWithChatRequests(metaId: Long): List<ContactWithChatRequest> {
        return dao.getContactsWithChatRequestsForAccount(metaId)
            .map { it.toDomain() }
    }

    override fun subscribePendingFanOutContacts(): Flow<List<Contact>> {
        return dao.subscribePendingFanOutContacts()
            .map { contacts -> contacts.map { it.toDomain() } }
    }

    override suspend fun markDevicesFannedOut(accountId: AccountId) {
        dao.markDevicesFannedOut(accountId.value)
    }
}
