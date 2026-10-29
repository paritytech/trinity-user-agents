package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.acceptance

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatAcceptanceRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliveryAccount
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliveryKeypairDerivation
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliverySigner
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.renewal.OneShotDeliveryRenewer
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.renewal.RenewableDelivery
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.renewalPeriod
import javax.inject.Inject

/**
 * Keeps published acceptances on the identity route until the contact shows up on its device route, which removes
 * them, or the contact is gone (chat-spec RFC-0005 §2).
 */
class ChatAcceptanceRenewer @Inject constructor(
    private val chatAcceptanceRepository: ChatAcceptanceRepository,
    private val contactsRepository: ContactsRepository,
    private val keypairDerivation: ChatRequestDeliveryKeypairDerivation,
    private val publisher: ChatAcceptancePublisher,
    private val renewer: OneShotDeliveryRenewer,
) {
    suspend fun renew(): Result<Unit> = runCancellableCatching {
        chatAcceptanceRepository.getDelivered().mapNotNull { toCandidate(it) }
    }.flatMap { candidates -> renewer.renew(candidates) }

    private suspend fun toCandidate(acceptance: ChatAcceptance): RenewableDelivery? {
        val period = acceptance.delivery.renewalPeriod ?: return null
        val contact = contactsRepository.getContact(acceptance.contactAccountId) ?: return null

        return AcceptanceRenewal(contact, acceptance, period)
    }

    private inner class AcceptanceRenewal(
        override val contact: Contact,
        private val acceptance: ChatAcceptance,
        override val deliveredPeriod: UInt,
    ) : RenewableDelivery {
        override val description: String = "Acceptance of ${acceptance.requestId}"

        override val deliveredByUsername: Boolean = acceptance.delivery is ChatRequest.Delivery.DeliveredByUsername

        override suspend fun account(period: UInt): Result<ChatRequestDeliveryAccount> {
            return keypairDerivation.acceptanceAccount(acceptance.requestId, period)
        }

        override suspend fun isStoredBy(signer: AccountId): Result<Boolean> {
            return publisher.isStoredBy(contact, signer)
        }

        override suspend fun republish(signer: ChatRequestDeliverySigner): Result<Unit> {
            return publisher.republish(contact, acceptance, signer)
        }

        override suspend fun markFailed(): Result<Unit> {
            return publisher.markFailed(acceptance)
        }
    }
}
