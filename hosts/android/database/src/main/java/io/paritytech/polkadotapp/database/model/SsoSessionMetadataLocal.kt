package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.ForeignKey
import androidx.room.Index

@Entity(
    tableName = "sso_session_metadata",
    primaryKeys = ["sessionSharedSecretPublicKey", "key"],
    foreignKeys = [
        ForeignKey(
            entity = SsoSessionLocal::class,
            parentColumns = ["sharedSecretPublicKey"],
            childColumns = ["sessionSharedSecretPublicKey"],
            onDelete = ForeignKey.CASCADE,
        ),
    ],
    indices = [Index(value = ["sessionSharedSecretPublicKey"])],
)
class SsoSessionMetadataLocal(
    val sessionSharedSecretPublicKey: ByteArray,
    val key: String,
    val value: String,
)
