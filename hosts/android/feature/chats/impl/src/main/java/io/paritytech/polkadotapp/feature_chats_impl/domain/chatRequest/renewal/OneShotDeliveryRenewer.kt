package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.renewal

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flattenResult
import io.paritytech.polkadotapp.common.utils.forEachAsync
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapAsync
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ALLOCATION_WAIT_TIMEOUT
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliveryAccount
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliveryAccountResolver
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliverySigner
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliverySigners
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.endsDelivery
import io.paritytech.polkadotapp.feature_statement_store_api.domain.notificationAllocator.NotificationAllocationError
import io.paritytech.polkadotapp.feature_statement_store_api.domain.notificationAllocator.NotificationStatementAccountAllocator
import timber.log.Timber
import javax.inject.Inject

/**
 * A chat request or acceptance kept on the store as one statement per period, signed by that period's notification
 * account or, with the user's consent, by our own account ([deliveredByUsername]).
 */
interface RenewableDelivery {
    val description: String
    val contact: Contact
    val deliveredPeriod: UInt
    val deliveredByUsername: Boolean

    suspend fun account(period: UInt): Result<ChatRequestDeliveryAccount>

    suspend fun isStoredBy(signer: AccountId): Result<Boolean>

    suspend fun republish(signer: ChatRequestDeliverySigner): Result<Unit>

    suspend fun markFailed(): Result<Unit>
}

/**
 * chat-spec RFC-0005 §2 and §4. A delivery from an earlier period is published again in the current one from a fresh
 * notification account, even when our own account signed the last copy. Without a free slot our own account signs it
 * if the user allowed it (asking otherwise), while the old copy lives out the grace window; a declined one fails.
 * A delivery of the current period that went missing is published again by the account that signed it.
 */
class OneShotDeliveryRenewer @Inject constructor(
    private val allocator: NotificationStatementAccountAllocator,
    private val signers: ChatRequestDeliverySigners,
    private val accountResolver: ChatRequestDeliveryAccountResolver,
) {
    suspend fun renew(candidates: List<RenewableDelivery>): Result<Unit> {
        val period = allocator.currentPeriod()
        val (stale, current) = candidates.partition { it.deliveredPeriod < period }
        Timber.i("oneShotRenewal: ${stale.size} from earlier periods, ${current.size} of period $period")

        return renewStale(stale, period).flatMap { resendMissing(current) }
    }

    private suspend fun renewStale(stale: List<RenewableDelivery>, period: UInt): Result<Unit> {
        val renewals = stale.mapNotNull { candidate -> freshAccount(candidate, period)?.let { Renewal(candidate, it) } }
        if (renewals.isEmpty()) return Result.success(Unit)

        return claimSlots(renewals).map { claims ->
            publishOnceAllocated(claims.claimed, period)
            claims.unclaimed.forEachAsync { renewal -> renewFromOurAccount(renewal.delivery, period) }
        }
    }

    private suspend fun freshAccount(candidate: RenewableDelivery, period: UInt): ChatRequestDeliveryAccount? {
        return candidate.account(period)
            .onFailure { error -> if (error.endsDelivery()) failDelivery(candidate, error) }
            .logFailure("${candidate.description}: no account for period $period")
            .getOrNull()
    }

    private suspend fun claimSlots(renewals: List<Renewal>): Result<SlotClaims> {
        val targets = renewals.map { it.account.accountId }

        return allocator.initiateAllocations(targets).map { claimed ->
            Timber.i("oneShotRenewal: claimed slots for ${claimed.size}/${targets.size} stale deliveries")
            val (withSlot, withoutSlot) = renewals.partition { it.account.accountId in claimed }
            SlotClaims(claimed = withSlot, unclaimed = withoutSlot)
        }
    }

    private suspend fun publishOnceAllocated(claimed: List<Renewal>, period: UInt) {
        claimed.forEachAsync { renewal ->
            val (delivery, account) = renewal
            allocator.awaitAllocated(account.accountId, ALLOCATION_WAIT_TIMEOUT)
                .flatMap { delivery.republish(signers.anonymousSigner(account)) }
                .onSuccess { Timber.i("oneShotRenewal: ${delivery.description} renewed into period $period from ${account.accountId}") }
                .onFailure { error ->
                    if (error is NotificationAllocationError.NoFreeSlotInPeriod) {
                        renewFromOurAccount(delivery, period)
                    } else {
                        Timber.e(error, "${delivery.description}: renewal into period $period failed")
                    }
                }
        }
    }

    private suspend fun renewFromOurAccount(candidate: RenewableDelivery, period: UInt) {
        accountResolver.usernameSignerIfAllowed(candidate.contact, period)
            .flatMap { signer -> candidate.republish(signer) }
            .onSuccess { Timber.i("oneShotRenewal: ${candidate.description} renewed into period $period from our own account") }
            .onFailure { error ->
                if (error.endsDelivery()) {
                    failDelivery(candidate, error)
                } else {
                    Timber.i("oneShotRenewal: ${candidate.description} keeps its old copy: ${error.message}")
                }
            }
    }

    private suspend fun failDelivery(candidate: RenewableDelivery, error: Throwable) {
        Timber.i("oneShotRenewal: ${candidate.description} fails: ${error.message}")
        candidate.markFailed().logFailure("${candidate.description}: could not be marked failed")
    }

    private suspend fun resendMissing(current: List<RenewableDelivery>): Result<Unit> {
        return current.mapAsync { candidate -> resendIfMissing(candidate) }.flattenResult().coerceToUnit()
    }

    private suspend fun resendIfMissing(candidate: RenewableDelivery): Result<Unit> {
        return signerOfCurrentCopy(candidate)
            .flatMap { signer -> if (signer == null) Result.success(Unit) else resendIfMissingFrom(candidate, signer) }
            .logFailure("${candidate.description}: presence check or re-send failed")
    }

    // A copy our own account signed is re-sent only while the user still allows it; otherwise the next period tries
    // a notification account again.
    private suspend fun signerOfCurrentCopy(candidate: RenewableDelivery): Result<ChatRequestDeliverySigner?> {
        if (candidate.deliveredByUsername) {
            return accountResolver.consentedUsernameSigner(candidate.contact, candidate.deliveredPeriod)
        }

        return candidate.account(candidate.deliveredPeriod).map { account -> signers.anonymousSigner(account) }
    }

    private suspend fun resendIfMissingFrom(candidate: RenewableDelivery, signer: ChatRequestDeliverySigner): Result<Unit> {
        return candidate.isStoredBy(signer.accountId).flatMap { stored ->
            Timber.d("oneShotRenewal: ${candidate.description} stored=$stored (signer ${signer.accountId})")
            if (stored) return@flatMap Result.success(Unit)

            Timber.i("oneShotRenewal: ${candidate.description} missing from the store; re-sending")
            candidate.republish(signer)
        }
    }
}

private data class Renewal(val delivery: RenewableDelivery, val account: ChatRequestDeliveryAccount)

private class SlotClaims(val claimed: List<Renewal>, val unclaimed: List<Renewal>)
