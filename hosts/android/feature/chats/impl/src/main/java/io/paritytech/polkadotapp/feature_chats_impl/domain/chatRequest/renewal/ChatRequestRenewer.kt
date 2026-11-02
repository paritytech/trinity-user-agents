package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.renewal

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactWithChatRequest
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.OutgoingChatRequestService
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliveryAccount
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliveryKeypairDerivation
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliverySigner
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestPayloadLoader
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestPublisher
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.renewalPeriod
import javax.inject.Inject

/** Keeps not-yet-accepted chat requests on the statement store, one statement per period (chat-spec RFC-0005 §2). */
class ChatRequestRenewer @Inject constructor(
    private val contactsRepository: ContactsRepository,
    private val keypairDerivation: ChatRequestDeliveryKeypairDerivation,
    private val outgoingChatRequestService: OutgoingChatRequestService,
    private val payloadLoader: ChatRequestPayloadLoader,
    private val publisher: ChatRequestPublisher,
    private val renewer: OneShotDeliveryRenewer,
) {
    suspend fun renew(): Result<Unit> = runCancellableCatching {
        contactsRepository.getContactsWithRenewablePendingRequests().mapNotNull(::toCandidate)
    }.flatMap { candidates -> renewer.renew(candidates) }

    private fun toCandidate(entry: ContactWithChatRequest): RenewableDelivery? {
        val request = entry.pendingChatRequest ?: return null
        val period = request.delivery.renewalPeriod ?: return null

        return RequestRenewal(entry.contact, request, period)
    }

    private inner class RequestRenewal(
        override val contact: Contact,
        private val request: ChatRequest,
        override val deliveredPeriod: UInt,
    ) : RenewableDelivery {
        override val description: String = "Chat request ${request.id}"

        override val deliveredByUsername: Boolean = request.delivery is ChatRequest.Delivery.DeliveredByUsername

        override suspend fun account(period: UInt): Result<ChatRequestDeliveryAccount> {
            return keypairDerivation.deliveryAccount(request.id, period)
        }

        override suspend fun isStoredBy(signer: AccountId): Result<Boolean> {
            return outgoingChatRequestService.isStoredBy(contact, signer)
        }

        override suspend fun republish(signer: ChatRequestDeliverySigner): Result<Unit> {
            return payloadLoader.load(contact, request).flatMap { payload -> publisher.republish(request, payload, signer) }
        }

        override suspend fun markFailed(): Result<Unit> {
            return publisher.markFailed(request)
        }
    }
}
