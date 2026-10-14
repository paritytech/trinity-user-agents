package io.paritytech.polkadotapp.database.model

import androidx.room.ColumnInfo
import androidx.room.Embedded
import androidx.room.Entity
import androidx.room.ForeignKey
import androidx.room.Index
import androidx.room.PrimaryKey

@Entity(
    tableName = "file_uploads",
    indices = [
        Index(value = ["chatId"])
    ],
    foreignKeys = [
        ForeignKey(
            entity = ChatMessageLocal::class,
            parentColumns = ["id"],
            childColumns = ["messageId"],
            onDelete = ForeignKey.CASCADE
        ),
        ForeignKey(
            entity = ChatRoomLocal::class,
            parentColumns = ["id"],
            childColumns = ["chatId"],
            onDelete = ForeignKey.CASCADE
        )
    ]
)
class FileUploadLocal(
    @PrimaryKey val messageId: String,
    val chatId: ByteArray,
    val fileUri: String,
    val mimeType: String,
    val fileSize: Long?,
    val totalChunks: Int,
    val uploadedChunks: Int,
    val ticket: ByteArray,
    @ColumnInfo(defaultValue = "")
    val nodeUrl: String,
    /**
     * Comma-separated hex-encoded blake2b-256 hashes of uploaded encrypted chunks.
     * Example: "0xab12...,0xcd34...,0xef56..."
     */
    val chunkHashes: String?,
    val status: Status,
    val errorCategory: ErrorCategory?,
    val errorCause: String?,
    val createdAt: Long,
    @Embedded
    val retryState: TransferRetryStateLocal
) {
    enum class Status {
        PENDING, IN_PROGRESS, DONE, FAILED
    }

    enum class ErrorCategory {
        NETWORK,
        FILE_NOT_FOUND,
        HOP_ERROR,
        UNKNOWN
    }
}
