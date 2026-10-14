package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.SendRecipientLocal
import kotlinx.coroutines.flow.Flow

@Dao
interface SendRecipientDao {
    @Query("SELECT * FROM send_recipients")
    suspend fun getSendRecipients(): List<SendRecipientLocal>

    @Query("SELECT * FROM send_recipients WHERE chainId = :chainId AND chainAssetId = :assetId")
    fun getSendRecipientsForChainAssetFlow(chainId: String, assetId: Int): Flow<List<SendRecipientLocal>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertSendRecipient(recipient: SendRecipientLocal)
}
