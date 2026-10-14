package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import androidx.room.Transaction
import io.paritytech.polkadotapp.database.model.SsoSessionLocal
import io.paritytech.polkadotapp.database.model.SsoSessionMetadataLocal
import io.paritytech.polkadotapp.database.model.SsoSessionWithMetadata
import kotlinx.coroutines.flow.Flow

@Dao
abstract class SsoSessionDao {
    @Transaction
    @Query("SELECT * FROM sso_sessions")
    abstract fun observeAll(): Flow<List<SsoSessionWithMetadata>>

    @Query("SELECT COUNT(*) FROM sso_sessions")
    abstract fun observeSessionCount(): Flow<Int>

    @Transaction
    @Query("SELECT * FROM sso_sessions")
    abstract suspend fun getAll(): List<SsoSessionWithMetadata>

    @Transaction
    @Query("SELECT * FROM sso_sessions WHERE statementStorePublicKey = :statementStorePublicKey")
    abstract suspend fun getByStatementStorePublicKey(statementStorePublicKey: ByteArray): SsoSessionWithMetadata?

    @Transaction
    open suspend fun upsert(session: SsoSessionLocal, metadata: List<SsoSessionMetadataLocal>) {
        insertSession(session)
        deleteMetadataForSession(session.sharedSecretPublicKey)
        insertMetadata(metadata)
    }

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    protected abstract suspend fun insertSession(session: SsoSessionLocal)

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    protected abstract suspend fun insertMetadata(entries: List<SsoSessionMetadataLocal>)

    @Query("DELETE FROM sso_session_metadata WHERE sessionSharedSecretPublicKey = :sessionSharedSecretPublicKey")
    protected abstract suspend fun deleteMetadataForSession(sessionSharedSecretPublicKey: ByteArray)

    @Query("DELETE FROM sso_sessions WHERE sharedSecretPublicKey = :sharedSecretPublicKey")
    abstract suspend fun delete(sharedSecretPublicKey: ByteArray)

    @Query("DELETE FROM sso_sessions WHERE statementStorePublicKey = :statementStorePublicKey")
    abstract suspend fun deleteByStatementStorePublicKey(statementStorePublicKey: ByteArray)

    @Query("SELECT outgoingUpdateTime FROM sso_sessions WHERE statementStorePublicKey = :statementStorePublicKey")
    abstract suspend fun getOutgoingUpdateTime(statementStorePublicKey: ByteArray): Long?

    @Query("UPDATE sso_sessions SET outgoingUpdateTime = :timePoint WHERE statementStorePublicKey = :statementStorePublicKey")
    abstract suspend fun updateOutgoingUpdateTime(statementStorePublicKey: ByteArray, timePoint: Long)

    @Query("SELECT lastSyncOfferId FROM sso_sessions WHERE statementStorePublicKey = :statementStorePublicKey")
    abstract suspend fun getLastSyncOfferId(statementStorePublicKey: ByteArray): String?

    @Query("UPDATE sso_sessions SET lastSyncOfferId = :offerId WHERE statementStorePublicKey = :statementStorePublicKey")
    abstract suspend fun updateLastSyncOfferId(statementStorePublicKey: ByteArray, offerId: String)
}
