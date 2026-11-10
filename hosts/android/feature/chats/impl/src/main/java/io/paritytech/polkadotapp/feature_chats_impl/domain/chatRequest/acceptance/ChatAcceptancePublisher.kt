package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.acceptance

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_impl.data.model.toEncodedMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatAcceptanceRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliverySigner
import io.paritytech.polkadotapp.feature_chats_impl.domain.sessions.ChatIdentityRouteFactory
import io.paritytech.polkadotapp.feature_statement_store_api.domain.OneShotSessionRequestPublisher
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.EncodedMessage
import timber.log.Timber
import javax.inject.Inject

class AcceptanceMessageMissing(messageId: String) : Throwable("Acceptance message $messageId no longer exists")

/**
 * Puts our `DeviceChatAccepted` on the identity route as the only message of a request that nobody answers, signed by
 * the given signer instead of our identity account (chat-spec RFC-0005 §2).
 */
class ChatAcceptancePublisher @Inject constructor(
    private val oneShotPublisher: OneShotSessionRequestPublisher,
    private val identityRouteFactory: ChatIdentityRouteFactory,
    private val chatEngine: ChatEngine,
    private val chatAcceptanceRepository: ChatAcceptanceRepository,
    private val contactsRepository: ContactsRepository,
) {
    suspend fun publishFirstDelivery(contact: Contact, acceptance: ChatAcceptance, signer: ChatRequestDeliverySigner): Result<Unit> {
        return submit(contact, acceptance, signer).flatMap {
            runCancellableCatching {
                contactsRepository.withTransaction {
                    chatAcceptanceRepository.updateDelivery(acceptance, signer.delivery)
                    chatEngine.updateMessageStatus(acceptance.messageId, ChatMessage.Status.IS_SENT)
                }
            }
        }
    }

    suspend fun republish(contact: Contact, acceptance: ChatAcceptance, signer: ChatRequestDeliverySigner): Result<Unit> {
        return submit(contact, acceptance, signer).flatMap {
            runCancellableCatching { chatAcceptanceRepository.updateDelivery(acceptance, signer.delivery) }
        }
    }

    suspend fun markFailed(acceptance: ChatAcceptance): Result<Unit> = runCancellableCatching {
        contactsRepository.withTransaction {
            chatAcceptanceRepository.updateDelivery(acceptance, ChatRequest.Delivery.Failed)
            chatEngine.updateMessageStatus(acceptance.messageId, ChatMessage.Status.DELIVERY_FAILED)
        }
    }

    suspend fun isStoredBy(contact: Contact, signer: AccountId): Result<Boolean> {
        return identityRouteFactory.create(contact).flatMap { route ->
            oneShotPublisher.isStoredBy(route.localAccount, route.remoteAccount, route.encryption, signer)
        }
    }

    // The transport request id is the message id, so every renewal carries the same request.
    private suspend fun submit(contact: Contact, acceptance: ChatAcceptance, signer: ChatRequestDeliverySigner): Result<Unit> {
        Timber.i("chatAcceptance: publishing the acceptance of ${acceptance.requestId} as ${signer.delivery}")

        return loadMessage(contact, acceptance).flatMap { message ->
            identityRouteFactory.create(contact).flatMap { route ->
                oneShotPublisher.publish(
                    localAccount = route.localAccount,
                    remoteAccount = route.remoteAccount,
                    encryption = route.encryption,
                    requestId = acceptance.messageId,
                    messages = listOf(message),
                    prover = signer.prover,
                )
            }
        }
    }

    private suspend fun loadMessage(contact: Contact, acceptance: ChatAcceptance): Result<EncodedMessage> {
        val message = chatEngine.getMessageById(ChatId.fromContact(contact.accountId), acceptance.messageId)
        if (message == null) {
            Timber.w("chatAcceptance: message ${acceptance.messageId} is gone; dropping the acceptance")
            chatAcceptanceRepository.remove(contact.accountId)
            return Result.failure(AcceptanceMessageMissing(acceptance.messageId))
        }

        return message.toEncodedMessage()
    }
}
