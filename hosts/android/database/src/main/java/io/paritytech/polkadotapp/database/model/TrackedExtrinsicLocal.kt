package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.PrimaryKey
import io.paritytech.polkadotapp.database.model.TrackedExtrinsicLocal.Companion.TABLE_NAME

@Entity(tableName = TABLE_NAME)
class TrackedExtrinsicLocal(
    @PrimaryKey
    val tag: String,
    val chainId: String,
    val signedExtrinsic: String,
    val status: String,
    val blockHash: String?,
    val errorMessage: String?,
    val additional: ByteArray?,
    val createdAt: Long,
) {
    companion object {
        const val TABLE_NAME = "tracked_extrinsics"
    }
}
