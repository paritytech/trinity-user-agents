package io.paritytech.polkadotapp.database.model

import androidx.room.ColumnInfo
import androidx.room.Embedded
import androidx.room.Entity
import androidx.room.ForeignKey
import androidx.room.Index
import androidx.room.PrimaryKey

@Entity(
    tableName = "file_downloads",
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
class FileDownloadLocal(
    @PrimaryKey val messageId: String,
    val chatId: ByteArray,
    val identifier: ByteArray,
    val ticket: ByteArray,
    @ColumnInfo(defaultValue = "")
    val nodeUrl: String,
    val mimeType: String,
    val filePath: String?,
    @ColumnInfo(defaultValue = "UNRESOLVED")
    val payloadKind: PayloadKind,
    val downloadedChunks: Int,
    /**
     * Comma-separated hex-encoded blake2b-256 hashes of all chunks.
     * Populated once the root entry resolves as [PayloadKind.CHUNKED]. Use [downloadedChunks] as offset on resume.
     */
    val chunkHashes: String?,
    val status: Status,
    val errorCategory: ErrorCategory?,
    val errorCause: String?,
    val createdAt: Long,
    @Embedded
    val retryState: TransferRetryStateLocal
) {
    enum class PayloadKind {
        UNRESOLVED, INLINE, CHUNKED
    }

    enum class Status {
        PENDING, IN_PROGRESS, DONE, FAILED, CANCELLED
    }

    enum class ErrorCategory {
        NETWORK,
        HOP_ERROR,
        FILE_WRITE_ERROR,
        UNKNOWN
    }
}
