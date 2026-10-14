package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.SsoHandledRequestLocal

@Dao
interface SsoHandledRequestDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insert(local: SsoHandledRequestLocal)

    @Query("SELECT EXISTS(SELECT 1 FROM sso_handled_requests WHERE sessionId = :sessionId AND requestId = :requestId)")
    suspend fun isHandled(sessionId: String, requestId: String): Boolean
}
