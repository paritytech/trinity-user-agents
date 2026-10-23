package io.paritytech.polkadotapp.database.model

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.Index
import androidx.room.PrimaryKey
import java.math.BigInteger

/**
 * A payment in a room of the chat product, which keeps the room's messages itself. The host keeps what a native chat
 * keeps in the payment message: the coin keys, and the status that the coinage flow writes, keyed by the message id.
 *
 * An outgoing payment is written in the transaction that commits the handoff of its coins, before the host hands it to
 * the product, so a crash between the two sends the same payment again under the same id. [delivery] is null for an
 * incoming payment. [coinKeys] holds the hex of each key, comma separated.
 */
@Entity(
    tableName = "product_chat_payments",
    indices = [Index(value = ["chatId"]), Index(value = ["settled"])],
)
class ProductChatPaymentLocal(
    @PrimaryKey val messageId: String,
    val chatId: ByteArray,
    val incoming: Boolean,
    val timestamp: Long,
    val totalValue: BigInteger,
    val coinKeys: String,
    val delivery: Delivery?,
    @ColumnInfo(defaultValue = "0") val refusals: Int,
    val status: Status,
    val statusAmount: BigInteger?,
    val settled: Boolean,
) {
    enum class Delivery {
        SENDING,
        DELIVERED,

        /** The product would not take the payment; it does not go again. */
        REFUSED,
    }

    /** The status of a native payment message (`ChatMessage.Content.CoinagePayment.Status`). */
    enum class Status {
        DETECTING,
        DETECTED,
        PARTIALLY_CLAIMED,
        TRANSFERRED,
        FAILED_DETECTION,
        FAILED_TRANSFER,
    }
}
