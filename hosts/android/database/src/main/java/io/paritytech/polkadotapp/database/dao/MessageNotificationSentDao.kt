package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.MessageNotificationSentLocal

@Dao
interface MessageNotificationSentDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insert(local: MessageNotificationSentLocal)

    @Query("SELECT EXISTS(SELECT 1 FROM message_notification_sent WHERE messageId = :messageId)")
    suspend fun wasNotificationSent(messageId: String): Boolean
}
