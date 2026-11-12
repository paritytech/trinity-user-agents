package io.paritytech.polkadotapp.feature_chats_impl.domain.extension

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtensionContext
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.markMessageProcessed
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage.Content.CoinagePayment
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage.Content.CoinagePayment.Status
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isIncoming
import io.paritytech.polkadotapp.feature_chats_api.domain.model.paymentContentOrNull
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.formatCoinsToBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinPrivateKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageTransferDetection
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.deriveKeypair
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.ClaimReceivedCoinsUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentState
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentStatusUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.isTerminal
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.lastOrNull
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.transformWhile
import kotlinx.coroutines.launch
import timber.log.Timber
import javax.inject.Inject
import kotlin.time.Duration.Companion.days
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

private const val COINAGE_LOG_TAG = "CoinageTransfer"

/** How long after a payment was sent its coins are still worth waiting for. */
private val CLAIM_RETRY_WINDOW = 7.days

/**
 * Keeps a payment message's status in step with the ledger, in both directions.
 *
 * Neither direction owns any state of its own. An outgoing payment is answered by our own coins' presence and
 * the status of whatever minted them; an incoming one by the group its claims were registered under, found
 * again from the message's id. That is why a restart mid-payment needs nothing to resume from: the message
 * says which coins, and the ledger says what happened to them.
 */
@OptIn(ExperimentalTime::class)
class CoinagePaymentProcessingExtension @Inject constructor(
    private val paymentStatusUseCase: CoinagePaymentStatusUseCase,
    private val claimReceivedCoinsUseCase: ClaimReceivedCoinsUseCase,
    private val balanceConverterUseCase: CoinageBalanceConverterUseCase,
) : ChatExtension {
    override val id = "CoinagePaymentProcessing"

    override val activationStateExternallyControlled: Boolean = false

    context(chatExtensionContext: ChatExtensionContext)
    override fun startGlobalWork() {
        chatExtensionContext.scope.launch {
            chatExtensionContext.getUnprocessedMessages(contentTypes = listOf(CoinagePayment::class))
                .forEach { message -> processPayment(message) }
        }

        chatExtensionContext.subscribeNewMessages(contentTypes = listOf(CoinagePayment::class))
            .onEach { message -> processPayment(message) }
            .launchIn(chatExtensionContext.scope)
    }

    context(chatExtensionContext: ChatExtensionContext)
    private fun processPayment(message: ChatMessage) {
        val content = message.paymentContentOrNull() ?: return

        Timber.tag(COINAGE_LOG_TAG)
            .d("Payment processing message=${message.id} incoming=${message.isIncoming} coins=${content.coinKeys.size}")

        chatExtensionContext.scope.launch {
            val coinKeys = content.coinKeys.map { it.toDataByteArray() }

            val outcome = if (message.isIncoming) {
                claimStatuses(message, content, coinKeys)
            } else {
                handoffStatuses(coinKeys)
            }
                .distinctUntilChanged()
                .onEach { status -> modifyPaymentContent(message, status) }
                .lastOrNull()

            // Reaching the end of the flow is the finish, not any status seen along the way. An incoming
            // payment's claim keeps retrying while a coin is still there to be claimed, so a partial
            // Transferred is a progress report; treating it as the end is what settled a 1.00 payment as
            // 0.12. A claim cut short by shutdown throws instead, leaving the message for the next launch.
            logSettled(message, outcome)
            with(chatExtensionContext) { markMessageProcessed(message) }
        }
    }

    /**
     * Ours to claim: submit under the message's own group, then report what the ledger makes of it.
     *
     * Coins that have not appeared are waited for until [CLAIM_RETRY_WINDOW] after the payment was sent. It is
     * measured from the message rather than from now so that reopening the app cannot extend it, and it only
     * closes once the chain has answered. Coins that are on chain are claimed whenever they are seen.
     */
    private fun claimStatuses(
        message: ChatMessage,
        content: CoinagePayment,
        coinKeys: List<CoinPrivateKey>,
    ): Flow<Status> {
        val groupId = claimGroupOf(message.id)
        // TODO: this deadline is persisted in every durable claim's submission params. If it is ever changed,
        //  add a migration that updates the stored deadlines too
        val retryUntil = Instant.fromEpochMilliseconds(message.timestamp) + CLAIM_RETRY_WINDOW

        Timber.tag(COINAGE_LOG_TAG)
            .i("Payment claiming message=${message.id} group=${groupId.value} coins=${coinKeys.size} until=$retryUntil")

        return claimReceivedCoinsUseCase.claim(coinKeys, groupId, retryUntil)
            .map { it.toPaymentStatus(expected = content.totalValue) }
    }

    /**
     * Ours until the peer takes them. Stops once every coin has been claimed or proven never to have existed,
     * since nothing after that can change.
     */
    private fun handoffStatuses(coinKeys: List<CoinPrivateKey>): Flow<Status> {
        val accountIds = coinKeys.map { it.deriveKeypair().publicKey.intoAccountId() }

        // Watched until every coin is beyond recall, not until it merely looks settled: a claim seen only
        // in a best-chain block can be forked away, and a closed message is one nothing is watching.
        return paymentStatusUseCase.subscribeStatuses(accountIds).transformWhile { states ->
            emit(states.toPaymentStatus())

            states.isEmpty() || states.hasOngoing()
        }
    }

    /**
     * Whether anything about this payment could still change.
     *
     * Knowing nothing counts as ongoing. An empty read is not a payment with nothing left to do — it is one
     * we cannot yet say anything about, and treating the two alike would close it for good on the single
     * reading that carries no information.
     */
    private fun Map<AccountId, CoinagePaymentState>.hasOngoing(): Boolean {
        return values.any { !it.status.isTerminal }
    }

    /**
     * What the message says now, which is a different question from whether to keep watching.
     *
     * This reports what the best chain shows, so a payment reads as claimed the moment the peer takes it
     * rather than a whole finality later. Whether it may be *closed* is [hasOngoing]'s business, and that
     * one does wait for finality.
     */
    private suspend fun Map<AccountId, CoinagePaymentState>.toPaymentStatus(): Status {
        val claimed = coinsMatching { it is CoinagePaymentStatus.Claimed }
        val awaiting = coinsMatching { it is CoinagePaymentStatus.AwaitingClaim }

        // Coins the peer has yet to take: on chain waiting, or not yet minted. Not the same as unproven —
        // a claim seen in a best block has been taken, whether or not it can still be rolled back.
        val outstanding = coinsMatching {
            it is CoinagePaymentStatus.AwaitingClaim || it is CoinagePaymentStatus.Detecting
        }

        return when {
            isEmpty() -> Status.Detecting

            // Something is still to be taken. A large payment is minted by several transactions, so its
            // coins appear in waves; calling it transferred for the wave already taken makes the message
            // announce a shortfall against an amount the rest of it is still on its way to make up.
            outstanding.isNotEmpty() -> if (awaiting.isEmpty() && claimed.isEmpty()) {
                Status.Detecting
            } else {
                Status.Detected(valueOf(awaiting + claimed))
            }

            // Nothing left to take: what was minted has been taken and anything else never existed, so a
            // shortfall here is real.
            claimed.isNotEmpty() -> Status.Transferred(valueOf(claimed))

            else -> Status.FailedDetection
        }
    }

    private fun Map<AccountId, CoinagePaymentState>.coinsMatching(predicate: (CoinagePaymentStatus) -> Boolean) =
        values.filter { predicate(it.status) }.map { it.coin }

    private suspend fun valueOf(coins: List<Coin>) =
        balanceConverterUseCase.create()
            .map { it.formatCoinsToBalance(coins) }
            .logFailure("Failed to value payment coins")
            .getOrDefault(Balance.ZERO)

    context(chatExtensionContext: ChatExtensionContext)
    private suspend fun modifyPaymentContent(message: ChatMessage, status: Status) {
        val content = message.paymentContentOrNull() ?: return

        Timber.tag(COINAGE_LOG_TAG)
            .d("Payment status message=${message.id} incoming=${message.isIncoming} status=$status")

        chatExtensionContext.modifyMessage(message.chatId, message.id, content.copy(status = status))
    }

    private fun logSettled(message: ChatMessage, status: Status?) {
        val outcome = if (message.isIncoming) "claimed" else "collectedByPeer"
        val group = if (message.isIncoming) claimGroupOf(message.id).value else "none"

        Timber.tag(COINAGE_LOG_TAG).i("Payment settled message=${message.id} outcome=$outcome group=$group status=$status")
    }
}

/**
 * Derived from the message rather than stored, so a retry after process death rejoins the claims the previous
 * attempt registered instead of submitting a second set.
 */
private fun claimGroupOf(messageId: ChatMessageId) = CoinageOperationGroupId("chat-claim:$messageId")

/**
 * [expected] stands in for the amount while claiming is still under way: what is worth reporting then is the
 * whole payment we are working towards, not the nothing that has landed so far.
 */
private fun CoinageTransferDetection.toPaymentStatus(expected: Balance): Status = when (this) {
    is CoinageTransferDetection.Detecting -> Status.Detecting
    is CoinageTransferDetection.Claiming -> Status.Detected(expected)
    is CoinageTransferDetection.ClaimingRest -> Status.PartiallyClaimed(claimed)

    // Both are the last word, and Transferred is what the message reads as final. A shortfall shows because
    // the amount is less than the message's own, which is only ever true once claiming is over.
    is CoinageTransferDetection.Claimed -> Status.Transferred(amount)
    is CoinageTransferDetection.ClaimedPartially -> Status.Transferred(claimed)

    is CoinageTransferDetection.NotClaimed -> Status.FailedTransfer
}
