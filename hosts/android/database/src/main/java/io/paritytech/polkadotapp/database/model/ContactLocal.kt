package io.paritytech.polkadotapp.database.model

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(
    tableName = "contacts"
)
class ContactLocal(
    @PrimaryKey val accountId: ByteArray,
    val username: String?,
    val chatKey: ByteArray,
    val ourMetaAccountId: Long,
    val sharedSecretDerivationPath: String,
    val avatar: String?,
    val pin: String?,
    val pushId: ByteArray?,
    val pushToken: ByteArray?,
    val lastSharedPushToken: String?,
    val operatingSystem: OperatingSystem?,
    val voipPushToken: ByteArray?,
    @ColumnInfo(defaultValue = "0")
    val isPeerLeft: Boolean = false,
    @ColumnInfo(defaultValue = "0")
    val isBlocked: Boolean = false,
    @ColumnInfo(defaultValue = "contactChat")
    val origin: String = "contactChat",
    val chatRequestId: String?,
    @ColumnInfo(defaultValue = "1")
    val pendingDevicesFanOut: Boolean = true,
    @ColumnInfo(defaultValue = "0")
    val addedAt: Long,
    val establishedAt: Long? = null,
) {
    enum class OperatingSystem {
        ANDROID,
        IOS
    }
}
