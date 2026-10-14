package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.ForeignKey
import androidx.room.Index

@Entity(
    tableName = "contact_devices",
    foreignKeys = [
        ForeignKey(
            entity = ContactLocal::class,
            parentColumns = ["accountId"],
            childColumns = ["contactAccountId"],
            onDelete = ForeignKey.CASCADE
        )
    ],
    indices = [
        Index(value = ["contactAccountId"]),
    ],
    primaryKeys = ["contactAccountId", "statementAccountId"]
)
class ContactDeviceLocal(
    val contactAccountId: ByteArray,
    val statementAccountId: ByteArray,
    val encryptionPublicKey: ByteArray,
)
