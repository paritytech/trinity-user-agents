package io.paritytech.polkadotapp.database.model

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(tableName = "sso_sessions")
class SsoSessionLocal(
    @PrimaryKey val sharedSecretPublicKey: ByteArray,
    val statementStorePublicKey: ByteArray,
    @ColumnInfo(defaultValue = "0")
    val addedAt: Long,
    @ColumnInfo(defaultValue = "ACTIVE")
    val status: String,
    @ColumnInfo(defaultValue = "0")
    val lastUpdate: Long,
    @ColumnInfo(defaultValue = "NULL")
    val outgoingUpdateTime: Long?,
    @ColumnInfo(defaultValue = "NULL")
    val lastSyncOfferId: String?,
)
