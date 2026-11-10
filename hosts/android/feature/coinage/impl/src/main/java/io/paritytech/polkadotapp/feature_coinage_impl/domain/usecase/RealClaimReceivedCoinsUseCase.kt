package io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase

import io.novasama.substrate_sdk_android.encrypt.keypair.Keypair
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.chains.multiNetwork.connection.holdingConnection
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinPrivateKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageTransferDetection
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.deriveKeypair
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageInput
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionState
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.ClaimReceivedCoinsUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageAssetValueUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainCoinInfo
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogD
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogE
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogI
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogW
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.ReceiveChannel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.channelFlow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.produceIn
import kotlinx.coroutines.withTimeoutOrNull
import javax.inject.Inject
import kotlin.time.Duration.Companion.seconds
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

@OptIn(ExperimentalTime::class, ExperimentalCoroutinesApi::class)
class RealClaimReceivedCoinsUseCase @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    private val coinRepository: CoinRepository,
    private val transactionService: CoinageTransactionService,
    private val assetValueUseCase: CoinageAssetValueUseCase,
    private val submissionUseCase: CoinageTransferSubmissionUseCase,
    private val timeProvider: TimeProvider,
    private val chainConnectionRefCounter: ChainConnectionRefCounter,
) : ClaimReceivedCoinsUseCase {
    private companion object {
        /**
         * How long one pass waits for every coin to show up before claiming whatever it can see.
         *
         * A peer's coin has no ledger row of ours, so only the chain can say it exists, and one that never
         * arrives must not hold up the ones that did.
         *
         * Reading the chain like a queue means a look is consumed once, so this bounds only how long one
         * pass holds out for a complete set — never how long claiming goes on, which is the caller's window.
         */
        val DETECTION_TIMEOUT = 30.seconds

        const val CONNECTION_LABEL = "CoinClaim"
    }

    override fun claim(
        coinKeys: List<CoinPrivateKey>,
        groupId: CoinageOperationGroupId,
        retryUntil: Instant,
    ): Flow<CoinageTransferDetection> = channelFlow {
        send(CoinageTransferDetection.Detecting)

        val keys = coinKeys.associateBy { key -> key.deriveKeypair().accountId() }

        coinageLogI("Claim starting group=${groupId.value} coins=${keys.size} until=$retryUntil")

        // Its a channel so we only process each on-chain state update once
        // To prevent a case where failing submit would cause us to loop at CPU speed (since awaitOnChainWithTimeout will remain the same)
        val onChain = subscribeCoinInfos(keys.keys.toList()).produceIn(this)

        var settled: List<CoinageTransactionState>
        var chainAnswered = false

        while (true) {
            settled = awaitKnownOperationsSettled(groupId, keys.keys, report = ::send)

            // Each coin is registered once. Rebuilding a claim that failed is its submission policy's job, into
            // the coin it recorded; a second claim here would mint into a coin nothing would ever wait on.
            val unregistered = keys.keys - settled.registeredCoins()

            // Every coin has a claim, and none of them can change any more. Claim finished.
            if (unregistered.isEmpty()) break

            val look = awaitOnChainWithTimeout(onChain, unregistered)
            if (look != null) chainAnswered = true

            val claimable = look.orEmpty()

            when {
                // Coin detected => register its claim
                claimable.isNotEmpty() -> submit(keys, claimable, groupId, retryUntil)

                // Timeout to limit claim of remaining coins in case they never appeared on-chain.
                // A chain that never answered says nothing about whether they did
                chainAnswered && timeProvider.now() >= retryUntil -> {
                    coinageLogW("Claim window closed group=${groupId.value} unregistered=${unregistered.size}")
                    break
                }

                else -> coinageLogD(
                    "Claim still waiting group=${groupId.value} unregistered=${unregistered.size} chainAnswered=$chainAnswered"
                )
            }
        }

        // Nothing further will be attempted, so this is the last word — and the only place a shortfall
        // may be called final. Logged like any other report: how a claim ended is the line worth having
        // when someone says they were paid less than they were sent.
        val verdict = settled.toVerdict(keys.keys)
        logDetection(groupId, verdict)
        send(verdict)

        // The chain subscription never ends by itself, and this flow does not finish while a child of its
        // scope is still running — so without this a finished claim would hang instead of completing, and
        // the caller would never learn it is done.
        onChain.cancel()
    }

    /**
     * Reports the group until nothing in it can change, then hands back what it settled on.
     *
     * A group with no entries is already settled — that is a first attempt, and there is nothing to wait for.
     */
    private suspend fun awaitKnownOperationsSettled(
        groupId: CoinageOperationGroupId,
        coins: Set<AccountId>,
        report: suspend (CoinageTransferDetection) -> Unit,
    ): List<CoinageTransactionState> {
        return transactionService.subscribeOperationGroupStatuses(groupId)
            .onEach { states ->
                val detection = states.toProgress(coins)
                logDetection(groupId, detection)
                report(detection)
            }
            .first { states -> states.none { it.status.isLive } }
    }

    /**
     * The next look at the chain that shows every coin still owed to us, or the best look taken within
     * [DETECTION_TIMEOUT] — null when the chain gave none at all.
     *
     * Holding out for all of them is deliberate: submitting while the sender's split is still landing is
     * what gets a claim refused. Settling for the last look is equally deliberate — a coin that never
     * arrives is the peer's problem, and holding the others hostage to it would strand money that is
     * sitting right there.
     */
    private suspend fun awaitOnChainWithTimeout(
        onChain: ReceiveChannel<Map<AccountId, OnChainCoinInfo>>,
        unclaimed: Set<AccountId>,
    ): Map<AccountId, OnChainCoinInfo>? {
        var latest: Map<AccountId, OnChainCoinInfo>? = null

        withTimeoutOrNull(DETECTION_TIMEOUT) {
            for (look in onChain) {
                // The newest look wins outright, even when it holds fewer coins than the one before: a fork
                // can take a coin away, and claiming against the widest view ever seen would submit against
                // one the chain no longer has.
                val current = look.filterKeys { it in unclaimed }
                latest = current

                if (current.keys.containsAll(unclaimed)) break
            }
        }

        return latest
    }

    private suspend fun submit(
        keys: Map<AccountId, CoinPrivateKey>,
        claimable: Map<AccountId, OnChainCoinInfo>,
        groupId: CoinageOperationGroupId,
        retryUntil: Instant,
    ) {
        coinageLogI("Claim submitting group=${groupId.value} claims=${claimable.size}")

        submissionUseCase(claimable.keys.mapNotNull(keys::get), claimable, groupId, retryUntil)
            .onFailure { coinageLogE("Claim submission failed group=${groupId.value}", it) }
    }

    /**
     * Coins with a claim of ours in a block: the reporting threshold.
     *
     * Counted over coins rather than entries, so a stale FAILURE row left behind by an earlier attempt stops
     * mattering once a later attempt for that coin lands. Counting entries is what left a retried payment
     * stuck on "claiming" no matter how much of it had arrived.
     */
    private fun List<CoinageTransactionState>.arrivedCoins(): Set<AccountId> =
        filter { it.status.isArrived }.receivedInputs()

    /**
     * Coins with a claim of ours that finalized: the threshold for stopping.
     *
     * Deliberately stricter than [arrivedCoins]. Report on inclusion, stop on finality — telling the user
     * the money is theirs a block after submission is the whole latency win, but giving up on a coin at that
     * point would leave it unclaimed for good if a fork took the block away.
     */
    private fun List<CoinageTransactionState>.finalizedCoins(): Set<AccountId> =
        filter { it.status == DurableTxStatus.FINALIZED_SUCCESS }.receivedInputs()

    /** Coins a claim of ours was registered for, whatever became of it. */
    private fun List<CoinageTransactionState>.registeredCoins(): Set<AccountId> = receivedInputs()

    /** Coins whose claim failed, or is being rebuilt after failing, rather than simply waiting on a block. */
    private fun List<CoinageTransactionState>.failedCoins(): Set<AccountId> =
        filter { it.status == DurableTxStatus.FAILURE || it.status == DurableTxStatus.PENDING_SUBMISSION }
            .receivedInputs()

    private fun List<CoinageTransactionState>.receivedInputs(): Set<AccountId> =
        flatMap { it.inputs }
            .filterIsInstance<CoinageInput.Coin.Received>()
            .mapTo(mutableSetOf()) { it.publicKey }

    /**
     * What is true right now, reported on every ledger update.
     *
     * Nothing here may say claiming is over — only the caller's loop knows that — so a shortfall is never
     * announced while another attempt could still make it up.
     */
    private suspend fun List<CoinageTransactionState>.toProgress(coins: Set<AccountId>): CoinageTransferDetection {
        val arrived = filter { it.status.isArrived }
        val outstanding = coins - arrivedCoins()

        return when {
            outstanding.isEmpty() ->
                CoinageTransferDetection.Claimed(valueMintedBy(arrived), finalized = (coins - finalizedCoins()).isEmpty())

            // Part of the payment is settled and the rest is waiting on a retry. Only worth saying when a
            // claim actually failed: on the happy path claims land one at a time, and reporting the running
            // total would walk the user through every inclusion of a payment that is simply in progress.
            arrived.isNotEmpty() && outstanding.any { it in failedCoins() } ->
                CoinageTransferDetection.ClaimingRest(valueMintedBy(arrived))

            isEmpty() -> CoinageTransferDetection.Detecting

            else -> CoinageTransferDetection.Claiming
        }
    }

    /**
     * The last word, once nothing further will be attempted.
     *
     * The same two questions as [toProgress], answered in the past tense — which is the only place a
     * shortfall may be called final.
     */
    private suspend fun List<CoinageTransactionState>.toVerdict(coins: Set<AccountId>): CoinageTransferDetection {
        val arrived = filter { it.status.isArrived }
        val notArrived = coins - arrivedCoins()

        return when {
            notArrived.isEmpty() ->
                CoinageTransferDetection.Claimed(valueMintedBy(arrived), finalized = (coins - finalizedCoins()).isEmpty())

            arrived.isNotEmpty() -> CoinageTransferDetection.ClaimedPartially(valueMintedBy(arrived))

            else -> CoinageTransferDetection.NotClaimed
        }
    }

    private suspend fun valueMintedBy(states: List<CoinageTransactionState>) =
        assetValueUseCase.valueOf(states.flatMap { it.outputs })
            .logFailure("Failed to value claimed coins")
            .getOrDefault(Balance.ZERO)

    private fun logDetection(groupId: CoinageOperationGroupId, detection: CoinageTransferDetection) {
        val group = groupId.value

        when (detection) {
            is CoinageTransferDetection.Detecting -> coinageLogD("Claim detecting group=$group")
            is CoinageTransferDetection.Claiming -> coinageLogD("Claim claiming group=$group")

            is CoinageTransferDetection.ClaimingRest ->
                coinageLogI("Claim partial group=$group claimed=${detection.claimed}")

            is CoinageTransferDetection.Claimed ->
                coinageLogI("Claim complete group=$group amount=${detection.amount} finalized=${detection.finalized}")

            is CoinageTransferDetection.ClaimedPartially ->
                coinageLogW("Claim ended short group=$group claimed=${detection.claimed}")

            is CoinageTransferDetection.NotClaimed -> coinageLogE("Claim failed group=$group")
        }
    }

    // A failed read ends the subscription while a claim may watch for days, so subscribe again. The connection
    // is held because claims run in the background, where a socket nothing holds is paused and never answers
    private fun subscribeCoinInfos(accountIds: List<AccountId>): Flow<Map<AccountId, OnChainCoinInfo>> {
        val chainId = chainAssetProvider.chainId()

        return flow {
            while (true) {
                val reads = coinRepository.subscribeCoinsInfoFor(chainId, accountIds)
                    .mapNotNull { read ->
                        read.logFailure("Can't fetch info for coins")
                            // Important: ignore failed reads via mapNotNull
                            .getOrNull()
                            ?.filterValuesNotNull()
                    }

                emitAll(reads)
                delay(DETECTION_TIMEOUT)
            }
        }.holdingConnection(chainConnectionRefCounter, chainId, CONNECTION_LABEL)
    }

    private fun Keypair.accountId(): AccountId = publicKey.toDataByteArray()
}

private fun <K, V : Any> Map<K, V?>.filterValuesNotNull(): Map<K, V> =
    mapNotNull { (key, value) -> value?.let { key to it } }.toMap()
