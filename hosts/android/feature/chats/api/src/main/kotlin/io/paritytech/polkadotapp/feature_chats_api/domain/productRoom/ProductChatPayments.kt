package io.paritytech.polkadotapp.feature_chats_api.domain.productRoom

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId

/**
 * The coinage payments of the person chats that the chat product runs (HX2). The coins, their keys and the claims stay
 * in the host, which keeps each payment under the id of its message, as the native chat keeps them in the message.
 * The product only carries the keys over its sessions.
 */
interface ProductChatPayments {
    /**
     * Pays [recipient] in the room of the chat product with that contact. [onSaved] runs inside the transaction that
     * keeps the payment, as a native payment message commits the handoff of its coins; when it throws, nothing is
     * kept. After that the host sends the payment to the product, again under the same id until the product has it.
     * Fails with [NoProductChatRoomException] when the user has no established chat with [recipient].
     */
    suspend fun send(recipient: AccountId, totalValue: Balance, coinKeys: List<ByteArray>, onSaved: suspend () -> Unit)

    /**
     * Keeps a payment that the contact of [chatId] sent, and claims its coins as the native chat claims a payment
     * message. Idempotent by [messageId]. [timestamp] is when the contact paid, in milliseconds since the Unix epoch.
     */
    suspend fun receive(chatId: ChatId, messageId: String, timestamp: Long, totalValue: Balance, coinKeys: List<ByteArray>)
}

/** The user has no established chat of the chat product with the recipient of a payment. */
class NoProductChatRoomException : Exception("no established chat with the recipient")
