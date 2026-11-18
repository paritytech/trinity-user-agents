package io.paritytech.polkadotapp.feature_chats_impl.domain.extension

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isIncoming
import io.paritytech.polkadotapp.feature_chats_api.domain.model.paymentContentOrNull
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageProcessingRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.MatchRendererCustomContentDecoder
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toDomain
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageExportId
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageHandOverPayments
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.HandedOverIncomingPayment
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.HandedOverOutgoingPayment
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.HandedOverPayments
import javax.inject.Inject
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

/**
 * The payment messages [CoinagePaymentProcessingExtension] has not settled yet, under the ids it follows them by,
 * so the wallet the app ran before hands them to the core.
 */
@OptIn(ExperimentalTime::class)
class ChatCoinageHandOverPayments @Inject constructor(
    private val processingRepository: ChatMessageProcessingRepository,
) : CoinageHandOverPayments {
    override suspend fun payments(): HandedOverPayments {
        val messages = processingRepository.getUnprocessedMessages(COINAGE_PAYMENT_EXTENSION_ID)
            // Payment content is built in, so no product renderer is needed to read it.
            .map { it.toDomain(MatchRendererCustomContentDecoder(emptyMap())) }
        val payments = messages.mapNotNull { message -> message.paymentContentOrNull()?.let { message to it } }

        return HandedOverPayments(
            outgoing = payments.filterNot { (message, _) -> message.isIncoming }.map { (message, content) ->
                HandedOverOutgoingPayment(
                    exportId = CoinageExportId.ofChatPayment(message.id),
                    coinKeys = content.coinKeys.map { it.toDataByteArray() },
                )
            },
            incoming = payments.filter { (message, _) -> message.isIncoming }.map { (message, content) ->
                HandedOverIncomingPayment(
                    claimGroupId = claimGroupOf(message.id),
                    claimBy = Instant.fromEpochMilliseconds(message.timestamp) + CLAIM_RETRY_WINDOW,
                    coinKeys = content.coinKeys.map { it.toDataByteArray() },
                )
            },
        )
    }
}
