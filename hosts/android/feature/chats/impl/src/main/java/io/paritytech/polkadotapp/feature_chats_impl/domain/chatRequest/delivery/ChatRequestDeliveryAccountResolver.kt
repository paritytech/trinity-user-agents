package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flatRecover
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_statement_store_api.domain.notificationAllocator.NotificationAllocationError
import io.paritytech.polkadotapp.feature_statement_store_api.domain.notificationAllocator.NotificationStatementAccountAllocator
import io.paritytech.polkadotapp.feature_statement_store_api.domain.notificationAllocator.allocate
import timber.log.Timber
import javax.inject.Inject
import kotlin.time.Duration.Companion.minutes

internal val ALLOCATION_WAIT_TIMEOUT = 5.minutes

/**
 * Picks the signer for a request's first delivery: a notification-funded account of the current period when a slot
 * can be had, otherwise our own account — no worse than before notification slots existed.
 */
class ChatRequestDeliveryAccountResolver @Inject constructor(
    private val allocator: NotificationStatementAccountAllocator,
    private val keypairDerivation: ChatRequestDeliveryKeypairDerivation,
    private val signers: ChatRequestDeliverySigners,
) {
    suspend fun resolveFirstDelivery(contact: Contact, request: ChatRequest): Result<ChatRequestDeliverySigner> {
        return anonymousSigner(request).flatRecover { error -> fallBackIfNoSlot(contact, error) }
    }

    private suspend fun anonymousSigner(request: ChatRequest): Result<ChatRequestDeliverySigner> {
        val period = allocator.currentPeriod()

        return keypairDerivation.deliveryAccount(request.id, period).flatMap { account ->
            Timber.i("chatRequestDelivery: request ${request.id} claiming notification slot for ${account.accountId} in period $period")
            allocator.allocate(account.accountId, ALLOCATION_WAIT_TIMEOUT).map { signers.anonymousSigner(account) }
        }
    }

    private suspend fun fallBackIfNoSlot(contact: Contact, error: Throwable): Result<ChatRequestDeliverySigner> {
        if (error !is NotificationAllocationError.NoFreeSlotInPeriod) return Result.failure(error)

        Timber.i("chatRequestDelivery: no notification slot this period; falling back to our own account")
        return signers.usernameSigner(contact)
    }
}
