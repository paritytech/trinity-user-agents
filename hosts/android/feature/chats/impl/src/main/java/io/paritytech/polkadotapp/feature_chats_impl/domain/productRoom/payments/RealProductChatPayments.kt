package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.payments

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatRooms
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatContactStateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage.Content.CoinagePayment
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.NoProductChatRoomException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatPayments
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments.ProductAttachmentStageScheduler
import kotlinx.coroutines.flow.first
import timber.log.Timber
import java.util.UUID
import javax.inject.Inject

/**
 * Keeps the payments of the rooms of the chat product as the native chat keeps a payment message: an outgoing one with
 * the commit of its coins, under a UUID and the time the user paid, and an incoming one once. The outbox job of the room
 * sends an outgoing payment to the product with the messages of the room, in order, and again under the same id until
 * the product has it. `CoinagePaymentProcessingExtension` follows the coins of both.
 */
class RealProductChatPayments @Inject constructor(
    private val store: ProductChatPaymentStore,
    private val declarations: ChatRoomDeclarationsRepository,
    private val personChatRooms: PersonChatRooms,
    private val scheduler: ProductAttachmentStageScheduler,
) : ProductChatPayments {
    override suspend fun send(recipient: AccountId, totalValue: Balance, coinKeys: List<ByteArray>, onSaved: suspend () -> Unit) {
        val chatId = establishedRoom(recipient) ?: throw NoProductChatRoomException()
        val payment = ProductChatPayment(
            messageId = UUID.randomUUID().toString(),
            chatId = chatId,
            incoming = false,
            timestamp = System.currentTimeMillis(),
            totalValue = totalValue,
            coinKeys = coinKeys,
            delivery = ProductChatPayment.Delivery.SENDING,
            refusals = 0,
            status = CoinagePayment.Status.Detecting,
            settled = false,
        )
        store.insertSent(payment, onSaved)
        Timber.i("truapi.chat.payment %s kept for %d coins", payment.messageId, coinKeys.size)

        scheduler.schedule(chatId)
    }

    override suspend fun receive(chatId: ChatId, messageId: String, timestamp: Long, totalValue: Balance, coinKeys: List<ByteArray>) {
        // A payment with no keys has nothing to claim.
        if (coinKeys.isEmpty()) return

        val payment = ProductChatPayment(
            messageId = messageId,
            chatId = chatId,
            incoming = true,
            timestamp = timestamp,
            totalValue = totalValue,
            coinKeys = coinKeys,
            delivery = null,
            refusals = 0,
            status = CoinagePayment.Status.Detecting,
            settled = false,
        )
        store.insertReceived(payment)
        Timber.i("truapi.chat.payment %s received with %d coins", messageId, coinKeys.size)
    }

    /** Sends again the payments that wait for the product, for example after the app stopped before their job ran. */
    suspend fun resumeDeliveries() {
        store.roomsWithSending().forEach { scheduler.schedule(it) }
    }

    // As the native chat pays a contact with an established chat, the room is the one whose contact is established.
    private suspend fun establishedRoom(recipient: AccountId): ChatId? = declarations.observeContactRooms().first()
        .entries
        .firstOrNull { (chatId, declared) ->
            val contact = declared.contact
            personChatRooms.isPersonChatRoom(chatId) && contact?.account == recipient && contact.state == ChatContactStateModel.ESTABLISHED
        }
        ?.key
}
