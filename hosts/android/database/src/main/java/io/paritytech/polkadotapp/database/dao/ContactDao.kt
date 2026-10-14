package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Query
import androidx.room.Transaction
import androidx.room.Upsert
import io.paritytech.polkadotapp.database.model.ContactLocal
import io.paritytech.polkadotapp.database.model.ContactWithChatRequestLocal
import io.paritytech.polkadotapp.database.model.ContactWithRequestTimestampLocal
import kotlinx.coroutines.flow.Flow

@Dao
abstract class ContactDao {
    @Upsert
    abstract suspend fun upsert(contact: ContactLocal)

    @Query("SELECT * FROM contacts")
    abstract suspend fun getAll(): List<ContactLocal>

    @Query("SELECT * FROM contacts WHERE addedAt > :after")
    abstract suspend fun getAddedAfter(after: Long): List<ContactLocal>

    @Query("SELECT * FROM contacts WHERE accountId = :accountId LIMIT 1")
    abstract suspend fun getByAccountId(accountId: ByteArray): ContactLocal?

    @Query("SELECT * FROM contacts WHERE accountId = :accountId LIMIT 1")
    abstract fun subscribeByAccountId(accountId: ByteArray): Flow<ContactLocal?>

    @Query("SELECT * FROM contacts WHERE pushId = :pushId LIMIT 1")
    abstract suspend fun getByPushId(pushId: ByteArray): ContactLocal?

    @Query("SELECT * FROM contacts")
    abstract fun subscribeAll(): Flow<List<ContactLocal>>

    @Query("SELECT COUNT(*) FROM contacts")
    abstract fun observeContactCount(): Flow<Int>

    @Query("SELECT * FROM contacts WHERE isBlocked = 1")
    abstract fun subscribeBlocked(): Flow<List<ContactLocal>>

    @Query("SELECT EXISTS (SELECT 1 FROM contacts WHERE isBlocked = 1)")
    abstract fun subscribeHasBlockedContacts(): Flow<Boolean>

    @Query("UPDATE contacts SET pushId = :newPushId WHERE accountId = :accountId")
    abstract suspend fun updatePushId(accountId: ByteArray, newPushId: ByteArray)

    @Query("UPDATE contacts SET username = :username WHERE accountId = :accountId")
    abstract suspend fun updateUsername(accountId: ByteArray, username: String?)

    @Query(
        """
        UPDATE contacts
        SET pushToken = :pushToken,
            operatingSystem = :operatingSystem
        WHERE accountId = :accountId
    """
    )
    abstract suspend fun updatePushTokenAndOperatingSystem(
        accountId: ByteArray,
        pushToken: ByteArray,
        operatingSystem: ContactLocal.OperatingSystem?
    )

    @Query("UPDATE contacts SET lastSharedPushToken = :newPushToken WHERE accountId = :accountId")
    abstract suspend fun updateLastSharedPushToken(accountId: ByteArray, newPushToken: String)

    @Query("DELETE FROM contacts WHERE accountId = :accountId")
    abstract suspend fun delete(accountId: ByteArray)

    @Query("UPDATE contacts SET isPeerLeft = :isLeft WHERE accountId = :accountId")
    abstract suspend fun updatePeerLeft(accountId: ByteArray, isLeft: Boolean)

    @Query("UPDATE contacts SET isBlocked = :isBlocked WHERE accountId = :accountId")
    abstract suspend fun updateIsBlocked(accountId: ByteArray, isBlocked: Boolean)

    @Query("UPDATE contacts SET voipPushToken = :voipPushToken WHERE accountId = :accountId")
    abstract suspend fun updateVoipPushToken(accountId: ByteArray, voipPushToken: ByteArray)

    @Query("UPDATE contacts SET chatRequestId = :chatRequestId WHERE accountId = :accountId")
    abstract suspend fun updateChatRequestId(accountId: ByteArray, chatRequestId: String?)

    @Query("UPDATE contacts SET chatRequestId = NULL, establishedAt = :establishedAt WHERE accountId = :accountId")
    abstract suspend fun markChatRequestAccepted(accountId: ByteArray, establishedAt: Long)

    @Query("SELECT * FROM contacts WHERE establishedAt IS NOT NULL AND establishedAt > :after")
    abstract suspend fun getEstablishedAfter(after: Long): List<ContactLocal>

    @Query("SELECT * FROM contacts WHERE pendingDevicesFanOut = 1")
    abstract fun subscribePendingFanOutContacts(): Flow<List<ContactLocal>>

    @Query("UPDATE contacts SET pendingDevicesFanOut = 0 WHERE accountId = :accountId")
    abstract suspend fun markDevicesFannedOut(accountId: ByteArray)

    @Query(
        """
        SELECT * FROM contacts
        WHERE chatRequestId IS NOT NULL
        AND ourMetaAccountId = :metaAccountId
        """
    )
    abstract suspend fun getContactsWithPendingRequests(metaAccountId: Long): List<ContactLocal>

    @Query(
        """
        SELECT contacts.*, chat_requests.timestamp as requestTimestamp FROM contacts
        INNER JOIN chat_requests ON contacts.chatRequestId = chat_requests.id
        WHERE chat_requests.direction = 'INCOMING'
        AND chat_requests.status = 'PENDING'
        """
    )
    abstract fun subscribeContactsWithPendingIncomingRequests(): Flow<List<ContactWithRequestTimestampLocal>>

    @Query(
        """
        SELECT COUNT(*) FROM contacts
        INNER JOIN chat_requests ON contacts.chatRequestId = chat_requests.id
        WHERE chat_requests.direction = 'INCOMING'
        AND chat_requests.status = 'PENDING'
        """
    )
    abstract fun subscribePendingIncomingRequestsCount(): Flow<Int>

    @Query(
        """
        SELECT
            contacts.*,
            chat_requests.id as request_id,
            chat_requests.timestamp as request_timestamp,
            chat_requests.direction as request_direction,
            chat_requests.status as request_status
        FROM contacts
        LEFT JOIN chat_requests ON contacts.chatRequestId = chat_requests.id
        WHERE contacts.accountId = :accountId
        LIMIT 1
        """
    )
    abstract fun subscribeContactWithChatRequest(accountId: ByteArray): Flow<ContactWithChatRequestLocal?>

    @Query(
        """
        SELECT
            contacts.*,
            chat_requests.id as request_id,
            chat_requests.timestamp as request_timestamp,
            chat_requests.direction as request_direction,
            chat_requests.status as request_status
        FROM contacts
        LEFT JOIN chat_requests ON contacts.chatRequestId = chat_requests.id
        """
    )
    abstract fun subscribeContactsWithChatRequests(): Flow<List<ContactWithChatRequestLocal>>

    @Query(
        """
        SELECT
            contacts.*,
            chat_requests.id as request_id,
            chat_requests.timestamp as request_timestamp,
            chat_requests.direction as request_direction,
            chat_requests.status as request_status
        FROM contacts
        LEFT JOIN chat_requests ON contacts.chatRequestId = chat_requests.id
        WHERE contacts.ourMetaAccountId = :metaAccountId
        """
    )
    abstract suspend fun getContactsWithChatRequestsForAccount(metaAccountId: Long): List<ContactWithChatRequestLocal>

    @Transaction
    open suspend fun withTransaction(action: suspend () -> Unit) {
        return action()
    }
}
