package io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.TopUpRepository
import io.paritytech.polkadotapp.feature_products_impl.data.storage.TopUpSourceStorage
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.onCompletion
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.ExperimentalTime

private const val COINAGE_LOG_TAG = "CoinageTransfer"

/**
 * The top-ups products have asked for, and how far each has got.
 *
 * Registering and following are separate because a top-up outlives the call that asked for it: the product
 * is told the operation exists, and then watches it — possibly from a later process, possibly after being
 * killed halfway through.
 */
interface TopUpService {
    /**
     * Records the top-up and starts it, returning as soon as it is registered rather than when it finishes.
     *
     * Fails with [TopUpError.AlreadyExists] if [id] is taken, [TopUpError.SourceBusy] if another top-up is
     * still drawing on [source], and [TopUpError.InvalidSource] if the source cannot be resolved.
     */
    suspend fun start(
        productId: ProductId,
        id: PaymentTopUpId,
        amount: Balance,
        source: PaymentTopUpSource,
    ): Result<Unit>

    /**
     * Follows a registered top-up, resuming it first if this process has not been running it.
     *
     * Fails with [TopUpError.NotFound] for an id this product never registered. Never completes on its own:
     * a terminal status stands until the product stops listening.
     */
    fun status(productId: ProductId, id: PaymentTopUpId): Flow<TopUpStatus>

    /**
     * Picks up every top-up left unfinished by a previous run. Called once at start-up.
     *
     * Without it a top-up only resumes when a product happens to ask after it, which leaves money sitting on
     * a key for as long as that product goes unopened — and the window would expire meanwhile, turning a
     * top-up that was merely interrupted into one that terminally failed.
     */
    suspend fun resumeUnfinished()
}

@Singleton // Important - stateful service!
@OptIn(ExperimentalTime::class)
class RealTopUpService @Inject constructor(
    private val repository: TopUpRepository,
    private val sourceStorage: TopUpSourceStorage,
    private val executeTopUpUseCase: ExecuteTopUpUseCase,
    private val sourceResolver: TopUpSourceResolver,
    private val acknowledgements: TopUpAcknowledgementPresenter,
    private val timeProvider: TimeProvider,
    dispatchers: CoroutineDispatchers,
) : TopUpService {
    private val scope = CoroutineScope(SupervisorJob() + dispatchers.computation)

    /**
     * One runner per operation, so two followers watch the same run rather than starting a second one
     * against the same money.
     */
    private val running = mutableMapOf<String, StateFlow<TopUpStatus>>()
    private val runningGuard = Mutex()

    override suspend fun start(
        productId: ProductId,
        id: PaymentTopUpId,
        amount: Balance,
        source: PaymentTopUpSource,
    ): Result<Unit> = runningGuard.withLock {
        if (repository.get(productId, id) != null) {
            Timber.tag(COINAGE_LOG_TAG).w("Top-up id already taken product=${productId.value} id=${id.asHex()}")

            return Result.failure(TopUpError.AlreadyExists(id))
        }

        claimantOf(productId, source)?.let { busyWith ->
            Timber.tag(COINAGE_LOG_TAG).w("Top-up source busy product=${productId.value} busyWith=${busyWith.asHex()}")

            return Result.failure(TopUpError.SourceBusy(busyWith))
        }

        val operation = TopUpOperation(
            id = id,
            productId = productId,
            amount = amount,
            startedAt = timeProvider.now(),
            outcome = null,
        )

        return sourceResolver.resolve(productId, source)
            .recoverCatching { throw TopUpError.InvalidSource(it) }
            // Both recorded before a single transaction is built. A product told its top-up was accepted must
            // be able to ask after it again whatever becomes of this process, and to have it picked back up.
            .flatMap { resolved -> sourceStorage.put(operation.groupId, source).map { resolved } }
            .flatMap { resolved -> repository.insert(operation).map { resolved } }
            .onSuccess { resolved -> run(operation, resolved) }
            .map { }
    }

    override fun status(productId: ProductId, id: PaymentTopUpId): Flow<TopUpStatus> = flow {
        val statuses = runningGuard.withLock {
            val operation = repository.get(productId, id) ?: throw TopUpError.NotFound(id)

            running[operation.groupId.value] ?: attach(operation)
        }

        emitAll(statuses)
    }

    override suspend fun resumeUnfinished() {
        val unfinished = repository.unfinished()

        Timber.tag(COINAGE_LOG_TAG).i("Top-ups to resume on launch: ${unfinished.size}")

        unfinished.forEach { operation ->
            // Per operation, because one that can no longer be resumed stays unfinished for good: letting it
            // out of the sweep would cost every top-up recorded after it a resume, on this launch and each
            // one after.
            runCancellableCatching {
                runningGuard.withLock {
                    if (!running.containsKey(operation.groupId.value)) attach(operation)
                }
            }.onFailure { error ->
                Timber.tag(COINAGE_LOG_TAG)
                    .e(error, "Top-up failed to resume product=${operation.productId.value} id=${operation.id.asHex()}")
            }
        }
    }

    /**
     * The unfinished top-up already drawing on [source], if there is one.
     *
     * Only the unfinished ones are asked about — a top-up that has reached a verdict has let go of its money
     * and cannot race anybody for it — so this reads the few sources still held rather than every one ever
     * stored.
     */
    private suspend fun claimantOf(productId: ProductId, source: PaymentTopUpSource): PaymentTopUpId? {
        return repository.unfinished()
            .firstOrNull { operation ->
                val held = sourceStorage.get(operation.groupId) ?: return@firstOrNull false

                source.drawsOnSameFundsAs(held, sameProduct = operation.productId == productId)
            }
            ?.id
    }

    /**
     * Runs an unfinished top-up, or reports the verdict of one that is over.
     *
     * A verdict is read back exactly as it was written rather than re-derived: entries a fork can still move
     * would let a status change after it was called terminal, which the contract forbids.
     */
    private suspend fun attach(operation: TopUpOperation): StateFlow<TopUpStatus> {
        operation.outcome?.let { return MutableStateFlow(it) }

        // Unfinished but unrunnable: the source is the one thing another attempt cannot do without. What its
        // transactions came to is still on the ledger, and is the whole of what can be said about it.
        val source = sourceStorage.get(operation.groupId)
            ?: return MutableStateFlow(executeTopUpUseCase.statusOf(operation))

        Timber.tag(COINAGE_LOG_TAG)
            .i("Top-up resuming product=${operation.productId.value} id=${operation.id.asHex()}")

        return sourceResolver.resolve(operation.productId, source)
            .recoverCatching { throw TopUpError.InvalidSource(it) }
            .map { resolved -> run(operation, resolved) }
            .getOrElse { throw it }
    }

    private fun run(operation: TopUpOperation, source: TopUpSource): StateFlow<TopUpStatus> {
        val statuses = MutableStateFlow<TopUpStatus>(TopUpStatus.Detecting)
        running[operation.groupId.value] = statuses

        // On the service's own scope, not the caller's: the product is not waiting on this, and a top-up
        // must keep going while nothing is listening to it.
        scope.launch {
            executeTopUpUseCase.execute(operation, source)
                .onEach { statuses.value = it }
                .onCompletion { settle(operation, statuses.value) }
                .collect()
        }

        return statuses
    }

    /**
     * Writes the verdict and drops the source, which does two things at once: it bounds how long a product's
     * secret keys are kept — they can only ever build another attempt, and there will not be another one —
     * and it fixes the status, so a top-up that ended is answerable for good and answers the same every time.
     */
    private suspend fun settle(operation: TopUpOperation, outcome: TopUpStatus) {
        if (!outcome.isTerminal) {
            // The run ended without a verdict — a chain that stalled past the window, say. The source stays,
            // so the next launch picks it up, by which time finality has almost certainly arrived.
            Timber.tag(COINAGE_LOG_TAG).w("Top-up ended without a verdict id=${operation.id.asHex()} last=$outcome")

            return
        }

        repository.settle(operation, outcome)
        sourceStorage.remove(operation.groupId)

        acknowledgements.acknowledge(operation, outcome)
    }
}

/**
 * Whether two sources are the same money, and so cannot be claimed at the same time.
 *
 * A derivation index means nothing outside the product whose subtree it indexes, so two of those are the
 * same source only when the product is too. Coin keys are the money itself rather than a way to reach it, so
 * a single key in common is enough: both top-ups would submit a claim for that coin and one would be refused.
 */
private fun PaymentTopUpSource.drawsOnSameFundsAs(other: PaymentTopUpSource, sameProduct: Boolean): Boolean =
    when {
        this is PaymentTopUpSource.ProductAccount && other is PaymentTopUpSource.ProductAccount ->
            sameProduct && index == other.index

        this is PaymentTopUpSource.PrivateKey && other is PaymentTopUpSource.PrivateKey -> key == other.key

        this is PaymentTopUpSource.Coins && other is PaymentTopUpSource.Coins ->
            secretKeys.any { it in other.secretKeys }

        else -> false
    }
