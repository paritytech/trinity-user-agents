package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flatRecover
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequestId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_impl.domain.identitySigning.IdentitySigningGate
import io.paritytech.polkadotapp.feature_statement_store_api.domain.notificationAllocator.NotificationAllocationError
import io.paritytech.polkadotapp.feature_statement_store_api.domain.notificationAllocator.NotificationStatementAccountAllocator
import io.paritytech.polkadotapp.feature_statement_store_api.domain.notificationAllocator.allocate
import timber.log.Timber
import javax.inject.Inject
import kotlin.time.Duration.Companion.minutes

internal val ALLOCATION_WAIT_TIMEOUT = 5.minutes

/**
 * Picks the signer of a request's or an acceptance's first delivery: a notification-funded account of the current
 * period when a slot can be had, otherwise our own account, but only once the user allowed it for this chat.
 */
class ChatRequestDeliveryAccountResolver @Inject constructor(
    private val allocator: NotificationStatementAccountAllocator,
    private val keypairDerivation: ChatRequestDeliveryKeypairDerivation,
    private val signers: ChatRequestDeliverySigners,
    private val identitySigningGate: IdentitySigningGate,
) {
    suspend fun resolveFirstDelivery(contact: Contact, request: ChatRequest): Result<ChatRequestDeliverySigner> {
        return resolve(contact, request.id) { period -> keypairDerivation.deliveryAccount(request.id, period) }
    }

    suspend fun resolveAcceptance(contact: Contact, requestId: ChatRequestId): Result<ChatRequestDeliverySigner> {
        return resolve(contact, requestId) { period -> keypairDerivation.acceptanceAccount(requestId, period) }
    }

    /** Our own account for a request or acceptance that got no slot in [period], asking the user unless they answered. */
    suspend fun usernameSignerIfAllowed(contact: Contact, period: UInt): Result<ChatRequestDeliverySigner> {
        return identitySigningGate.ensureConsent(contact.accountId).flatMap { signers.usernameSigner(contact, period) }
    }

    /** Our own account when the user already allowed it for this chat; null otherwise. Never asks. */
    suspend fun consentedUsernameSigner(contact: Contact, period: UInt): Result<ChatRequestDeliverySigner?> {
        if (!identitySigningGate.hasConsent(contact.accountId)) return Result.success(null)

        return signers.usernameSigner(contact, period)
    }

    private suspend fun resolve(
        contact: Contact,
        requestId: ChatRequestId,
        deriveAccount: suspend (period: UInt) -> Result<ChatRequestDeliveryAccount>,
    ): Result<ChatRequestDeliverySigner> {
        val period = allocator.currentPeriod()

        return anonymousSigner(requestId, period, deriveAccount).flatRecover { error -> fallBackIfNoSlot(contact, period, error) }
    }

    private suspend fun anonymousSigner(
        requestId: ChatRequestId,
        period: UInt,
        deriveAccount: suspend (period: UInt) -> Result<ChatRequestDeliveryAccount>,
    ): Result<ChatRequestDeliverySigner> {
        return deriveAccount(period).flatMap { account ->
            Timber.i("chatRequestDelivery: request $requestId claiming notification slot for ${account.accountId} in period $period")
            allocator.allocate(account.accountId, ALLOCATION_WAIT_TIMEOUT).map { signers.anonymousSigner(account) }
        }
    }

    private suspend fun fallBackIfNoSlot(contact: Contact, period: UInt, error: Throwable): Result<ChatRequestDeliverySigner> {
        if (error !is NotificationAllocationError.NoFreeSlotInPeriod) return Result.failure(error)

        Timber.i("chatRequestDelivery: no notification slot this period; our own account needs the user's consent")
        return usernameSignerIfAllowed(contact, period)
    }
}
