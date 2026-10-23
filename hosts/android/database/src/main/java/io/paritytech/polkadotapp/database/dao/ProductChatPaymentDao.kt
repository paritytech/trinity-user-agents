package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import androidx.room.Transaction
import io.paritytech.polkadotapp.database.model.ProductChatPaymentLocal
import kotlinx.coroutines.flow.Flow
import java.math.BigInteger

@Dao
abstract class ProductChatPaymentDao {
    /**
     * [onSaved] runs inside the same transaction as the insert, as a native payment message commits the handoff of its
     * coins: neither can be rolled back without the other.
     */
    @Transaction
    open suspend fun insertSent(payment: ProductChatPaymentLocal, onSaved: suspend () -> Unit) {
        insert(payment)
        onSaved()
    }

    @Insert(onConflict = OnConflictStrategy.ABORT)
    protected abstract suspend fun insert(payment: ProductChatPaymentLocal)

    /** Keeps the first record of a message id, which is what makes receiving a payment idempotent. */
    @Insert(onConflict = OnConflictStrategy.IGNORE)
    abstract suspend fun insertIfAbsent(payment: ProductChatPaymentLocal): Long

    @Query("SELECT * FROM product_chat_payments WHERE messageId = :messageId")
    abstract suspend fun get(messageId: String): ProductChatPaymentLocal?

    @Query("SELECT * FROM product_chat_payments WHERE chatId = :chatId")
    abstract fun observeRoom(chatId: ByteArray): Flow<List<ProductChatPaymentLocal>>

    @Query("SELECT * FROM product_chat_payments")
    abstract fun observeAll(): Flow<List<ProductChatPaymentLocal>>

    /** The payments whose status can still change. */
    @Query("SELECT * FROM product_chat_payments WHERE settled = 0")
    abstract fun observeUnsettled(): Flow<List<ProductChatPaymentLocal>>

    @Query("SELECT * FROM product_chat_payments WHERE chatId = :chatId AND delivery = 'SENDING' ORDER BY timestamp")
    abstract suspend fun getSending(chatId: ByteArray): List<ProductChatPaymentLocal>

    @Query("SELECT DISTINCT chatId FROM product_chat_payments WHERE delivery = 'SENDING'")
    abstract suspend fun getRoomsWithSending(): List<ByteArray>

    @Query("UPDATE product_chat_payments SET delivery = :delivery WHERE messageId = :messageId")
    abstract suspend fun setDelivery(messageId: String, delivery: ProductChatPaymentLocal.Delivery)

    @Query("UPDATE product_chat_payments SET refusals = refusals + 1 WHERE messageId = :messageId")
    abstract suspend fun countRefusal(messageId: String)

    @Query("UPDATE product_chat_payments SET status = :status, statusAmount = :amount WHERE messageId = :messageId")
    abstract suspend fun setStatus(messageId: String, status: ProductChatPaymentLocal.Status, amount: BigInteger?)

    @Query("UPDATE product_chat_payments SET settled = 1 WHERE messageId = :messageId")
    abstract suspend fun settle(messageId: String)
}
