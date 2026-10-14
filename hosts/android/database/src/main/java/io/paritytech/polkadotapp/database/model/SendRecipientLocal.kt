package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(
    tableName = "send_recipients"
)
class SendRecipientLocal(
    @PrimaryKey
    val accountId: ByteArray,
    val chainId: String,
    val chainAssetId: Int,
    val createdAt: Long,
)
