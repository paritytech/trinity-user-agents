package io.paritytech.polkadotapp.feature_chats_impl.domain.export

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.NativeChatPaymentAdoption
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.contactOrNull
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isIncoming
import io.paritytech.polkadotapp.feature_chats_api.domain.model.paymentContentOrNull
import io.paritytech.polkadotapp.feature_chats_impl.data.migration.NativeChatReader
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.AlwaysFailCustomContentDecoder
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.extension.CoinagePaymentProcessingExtension
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.payments.ProductChatPayment
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.payments.ProductChatPaymentStore
import timber.log.Timber
import javax.inject.Inject
import kotlin.time.Instant

/**
 * The native chat kept a payment in its message: the coin keys, and the status that [CoinagePaymentProcessingExtension]
 * wrote, with a processing mark once the coinage flow of the message was over. A payment record of the chat product
 * holds the same, so each payment message of a contact chat becomes one under the same id, time, amount, keys and
 * status. One the native flow finished is settled and never followed again. Any other one is followed by the same flow
 * on its record: an incoming payment is claimed under the same group (`chat-claim:<message id>`) and in the same window
 * (7 days from the message), so a claim the native flow started is joined rather than made twice. The chat product
 * already has the message from the history import, so an outgoing payment is kept as delivered and never sent again.
 */
class RealNativeChatPaymentAdoption @Inject constructor(
    private val reader: NativeChatReader,
    private val chatMessageRepository: ChatMessageRepository,
    private val store: ProductChatPaymentStore,
) : NativeChatPaymentAdoption {
    override suspend fun adopt(roomOf: (AccountId) -> ChatId): Int {
        val unfinished = reader.unprocessedMessageIds(CoinagePaymentProcessingExtension.ID)
        val payments = chatMessageRepository.getMessagesUpdatedAfter(Instant.DISTANT_PAST, AlwaysFailCustomContentDecoder())
            .mapNotNull { message -> message.toPayment(roomOf, unfinished) }
        val kept = store.adopt(payments)
        Timber.i("truapi.chat.payment adopted %d of %d native payments", kept, payments.size)
        return kept
    }

    private fun ChatMessage.toPayment(roomOf: (AccountId) -> ChatId, unfinished: Set<String>): ProductChatPayment? {
        val contact = chatId.contactOrNull() ?: return null
        val content = paymentContentOrNull() ?: return null
        return ProductChatPayment(
            messageId = id,
            chatId = roomOf(contact.contactAccountId),
            incoming = isIncoming,
            timestamp = timestamp,
            totalValue = content.totalValue,
            coinKeys = content.coinKeys,
            delivery = if (isIncoming) null else ProductChatPayment.Delivery.DELIVERED,
            refusals = 0,
            status = content.status,
            // A payment without keys has nothing to claim or to follow.
            settled = id !in unfinished || content.coinKeys.isEmpty(),
        )
    }
}
