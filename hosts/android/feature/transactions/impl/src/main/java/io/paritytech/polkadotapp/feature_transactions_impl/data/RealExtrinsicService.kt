package io.paritytech.polkadotapp.feature_transactions_impl.data

import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericEvent
import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.novasama.substrate_sdk_android.runtime.extrinsic.signer.SendableExtrinsic
import io.paritytech.polkadotapp.chains.extrinsic.ExtrinsicStatus
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.InlineWithRuntime
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.getRuntime
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.ChainEventsRepositoryFactory
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.ExtrinsicWithEvents
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.findExtrinsicFailureOrThrow
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.getExtrinsicWithEvents
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.isSuccess
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.chains.network.rpc.RpcCalls
import io.paritytech.polkadotapp.chains.repository.ChainStateRepository
import io.paritytech.polkadotapp.chains.util.tip
import io.paritytech.polkadotapp.chains.util.utilityAsset
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.orZero
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.common.utils.takeWhileInclusive
import io.paritytech.polkadotapp.feature_transactions.api.data.DispatchError
import io.paritytech.polkadotapp.feature_transactions.api.data.EnrichedSendableExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicDispatch
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicExecutionResult
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.data.FormExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.data.FormMultiExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.data.SignerProvider
import io.paritytech.polkadotapp.feature_transactions.api.data.StoringMultiExtrinsicBuilder
import io.paritytech.polkadotapp.feature_transactions.api.data.bindDispatchError
import io.paritytech.polkadotapp.feature_transactions.api.data.extensions.DefaultTransactionExtensionProvider
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.SimpleAccountFee
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.SimpleFee
import io.paritytech.polkadotapp.feature_transactions.api.data.feeSignerOrThrow
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ExtrinsicRecoveryContext
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ExtrinsicRecoveryVerdict
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ExtrinsicSubmissionFailure
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ExtrinsicSubmissionFailureRecovery
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ExtrinsicSubmissionFailureRecoveryStrategy
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.PreSubmissionValidationFailed
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ResubmitWhenValidFactory
import io.paritytech.polkadotapp.feature_transactions.api.data.submissionSigner
import io.paritytech.polkadotapp.feature_transactions.api.data.withMortality
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.AccountFee
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.ExtrinsicSubmission
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.Fee
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.accountId
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.accountIdOrThrow
import io.paritytech.polkadotapp.feature_transactions_impl.data.validation.ExtrinsicValidator
import io.paritytech.polkadotapp.feature_transactions_impl.data.validation.PreSubmissionValidator
import io.paritytech.polkadotapp.feature_transactions_impl.data.validation.TransactionValidity
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.FlowCollector
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.withContext
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RealExtrinsicService @Inject constructor(
    private val rpcCalls: RpcCalls,
    private val chainRegistry: ChainRegistry,
    private val extrinsicBuilderFactory: ExtrinsicBuilderFactory,
    private val preSubmissionValidator: PreSubmissionValidator,
    private val extrinsicValidator: ExtrinsicValidator,
    private val chainStateRepository: ChainStateRepository,
    private val signerProvider: SignerProvider,
    private val chainEventsRepositoryFactory: ChainEventsRepositoryFactory,
    private val coroutineDispatchers: CoroutineDispatchers,
    private val resubmitWhenValidFactory: ResubmitWhenValidFactory,
    private val defaultExtensionProviders: Set<@JvmSuppressWildcards DefaultTransactionExtensionProvider>,
) : ExtrinsicService {
    override suspend fun submitExtrinsic(
        chain: Chain,
        origin: TransactionOrigin,
        options: ExtrinsicService.SubmissionOptions,
        formExtrinsic: FormExtrinsic
    ): Result<ExtrinsicSubmission> = withContext(coroutineDispatchers.io) {
        runCatching {
            val submission = buildSubmittableExtrinsic(chain, origin, options, formExtrinsic)
            val hash = rpcCalls.submitExtrinsic(chain.id, submission.extrinsic.extrinsicHex)

            ExtrinsicSubmission(hash)
        }
    }

    override suspend fun submitExtrinsicAndAwaitExecution(
        chain: Chain,
        origin: TransactionOrigin,
        options: ExtrinsicService.SubmissionOptions,
        submissionFailureRecovery: ExtrinsicSubmissionFailureRecoveryStrategy?,
        formExtrinsic: FormExtrinsic
    ): Result<ExtrinsicExecutionResult> {
        val recovery = submissionFailureRecovery ?: resubmitWhenValidFactory.createForTxInvalidation(chain.id, DEFAULT_RECOVERY_MAX_ATTEMPTS)
        return withContext(coroutineDispatchers.io) {
            submitAndWatchExtrinsic(chain, origin, options, recovery, formExtrinsic)
                .awaitInBlock()
                .map { determineExtrinsicOutcome(it, chain) }
        }
    }

    override fun submitAndWatchBuiltExtrinsic(
        chain: Chain,
        extrinsic: SendableExtrinsic,
        submissionFailureRecovery: ExtrinsicSubmissionFailureRecoveryStrategy
    ): Flow<ExtrinsicStatus> = flow {
        submitWithRecovery(chain, submissionFailureRecovery, extrinsic)
    }

    override suspend fun submitExtrinsicsAndAwaitInBlock(
        chain: Chain,
        options: ExtrinsicService.SubmissionOptions,
        submissionFailureRecovery: ExtrinsicSubmissionFailureRecoveryStrategy,
        formExtrinsic: FormMultiExtrinsic
    ): Result<List<Result<ExtrinsicStatus.InBlock>>> {
        // submissionFailureRecovery is not applied to multi-extrinsic batches: resubmitting one
        // nonce-sequenced extrinsic mid-batch needs dedicated handling and is out of scope here.
        return runCatching { buildSubmittableExtrinsicMulti(chain, options, formExtrinsic) }
            .mapCatching { submissions ->
                val allWork = coroutineScope {
                    submissions.mapIndexed { index, submission ->
                        Timber.d("Submitting tx ${index / submissions.size}")

                        async {
                            rpcCalls.submitAndWatchExtrinsic(chain.id, submission.extrinsic.extrinsicHex)
                                .onEach { Timber.d("Tx $index / ${submissions.size} got status: $it") }
                                .takeWhileInclusive { !it.terminal }
                                .awaitInBlock()
                        }
                    }
                }

                allWork.awaitAll()
            }
    }

    override suspend fun estimateFee(
        chain: Chain,
        origin: TransactionOrigin,
        options: ExtrinsicService.SubmissionOptions,
        formExtrinsic: FormExtrinsic,
    ): Result<AccountFee> {
        return runCatching {
            val originOrDefault = origin

            if (originOrDefault.paysFees.not()) {
                return@runCatching SimpleAccountFee(
                    origin = originOrDefault.signerSource.accountIdOrThrow(chain),
                    amount = 0.intoBalance(),
                    asset = chain.utilityAsset
                )
            }

            val factoryOptions = options.createExtrinsicFactoryOptions()

            val feeSigner = signerProvider.feeSignerOrThrow(originOrDefault.signerSource)

            val extrinsicBuilder = extrinsicBuilderFactory.createForFee(chain, feeSigner, factoryOptions)
            formExtrinsic(InlineWithRuntime(extrinsicBuilder.runtime), extrinsicBuilder)
            options.feePayment.modifyExtrinsic(extrinsicBuilder)
            originOrDefault.applyTo(extrinsicBuilder)
            val extrinsic = extrinsicBuilder.buildExtrinsic()

            val nativeFee = estimateNativeFee(chain, extrinsic)
            val convertedFee = options.feePayment.convertNativeFee(nativeFee)

            SimpleAccountFee(
                origin = originOrDefault.signerSource.accountIdOrThrow(chain),
                amount = convertedFee.amount,
                asset = convertedFee.asset
            )
        }
    }

    private fun submitAndWatchExtrinsic(
        chain: Chain,
        origin: TransactionOrigin,
        options: ExtrinsicService.SubmissionOptions,
        submissionFailureRecovery: ExtrinsicSubmissionFailureRecoveryStrategy,
        formExtrinsic: FormExtrinsic
    ): Flow<ExtrinsicStatus> = flow {
        val built = runCatching { buildSubmittableExtrinsic(chain, origin, options, formExtrinsic) }

        built.fold(
            // A build/sign failure has no SendableExtrinsic to recover, so it bypasses the recovery loop.
            onFailure = { emit(ExtrinsicStatus.FailedToSubmit(it)) },
            onSuccess = { initialSubmission -> submitWithRecovery(chain, submissionFailureRecovery, initialSubmission.extrinsic) }
        )
    }

    context(flowCollector: FlowCollector<ExtrinsicStatus>)
    private suspend fun submitWithRecovery(
        chain: Chain,
        submissionFailureRecovery: ExtrinsicSubmissionFailureRecoveryStrategy,
        extrinsicToSubmit: SendableExtrinsic
    ) {
        var extrinsic = extrinsicToSubmit
        // Only the very first submission is validated up-front; a resubmission handed back by a recovery
        // strategy is already known-valid (the strategy only resubmits once the runtime accepts the tx).
        var validateBeforeSubmission = true

        Timber.d("Submitting extrinsic on ${chain.id}")

        while (true) {
            val failedAttempt = preSubmissionFailure(chain, extrinsic, validateBeforeSubmission)
                ?: submitOnce(chain, extrinsic)

            validateBeforeSubmission = false

            if (failedAttempt == null) {
                Timber.d("Extrinsic submitted successfully on ${chain.id}")
                break
            }

            Timber.w("Submission attempt on ${chain.id} failed: ${failedAttempt.cause}")

            val recoveryContext = recoveryContextFor(chain, extrinsic)

            when (submissionFailureRecovery.recoverSubmissionFailure(recoveryContext, failedAttempt.cause)) {
                ExtrinsicSubmissionFailureRecovery.Abort -> {
                    Timber.w("Recovery aborted extrinsic submission on ${chain.id}")
                    flowCollector.emit(failedAttempt.terminalStatus)
                    break
                }

                // The same bytes, held here rather than handed to the strategy: recovery decides whether to
                // resubmit, never what to submit.
                ExtrinsicSubmissionFailureRecovery.Resubmit ->
                    Timber.i("Recovery is resubmitting the extrinsic on ${chain.id}")
            }
        }
    }

    /**
     * The only capability a recovery strategy gets: re-ask the runtime. It cannot submit, which is what keeps
     * [PreSubmissionValidationFailed] meaning "these bytes were never sent" no matter what strategy is used.
     */
    private fun recoveryContextFor(chain: Chain, extrinsic: SendableExtrinsic) = object : ExtrinsicRecoveryContext {
        override suspend fun revalidate(): Result<ExtrinsicRecoveryVerdict> {
            val blockHash = chainStateRepository.currentBlockHash(chain.id)
            val body = extrinsic.bytesWithoutLength.toDataByteArray()

            return extrinsicValidator.validate(chain.id, body, blockHash).map { it.toRecoveryVerdict() }
        }
    }

    private fun TransactionValidity.toRecoveryVerdict(): ExtrinsicRecoveryVerdict = when (this) {
        is TransactionValidity.Valid -> ExtrinsicRecoveryVerdict.ACCEPTED
        is TransactionValidity.Invalid ->
            if (isMortalityExpired) ExtrinsicRecoveryVerdict.EXPIRED else ExtrinsicRecoveryVerdict.REJECTED_FOR_NOW
        is TransactionValidity.Unknown -> ExtrinsicRecoveryVerdict.UNKNOWN
    }

    private suspend fun preSubmissionFailure(
        chain: Chain,
        extrinsic: SendableExtrinsic,
        validate: Boolean
    ): FailedAttempt? {
        if (!validate || preSubmissionValidator.mightBeValid(chain.id, extrinsic)) return null

        return FailedAttempt(
            cause = ExtrinsicSubmissionFailure.PreSubmissionValidation,
            terminalStatus = ExtrinsicStatus.FailedToSubmit(PreSubmissionValidationFailed())
        )
    }

    context(flowCollector: FlowCollector<ExtrinsicStatus>)
    private suspend fun submitOnce(chain: Chain, extrinsic: SendableExtrinsic): FailedAttempt? {
        var terminalFailure: ExtrinsicStatus.Failure? = null

        rpcCalls.submitAndWatchExtrinsic(chain.id, extrinsic.extrinsicHex)
            .takeWhileInclusive { !it.terminal }
            .collect { status ->
                if (status.terminal && status is ExtrinsicStatus.Failure) {
                    terminalFailure = status
                } else {
                    flowCollector.emit(status)
                }
            }

        return terminalFailure?.let { FailedAttempt(it.toSubmissionFailure(), it as ExtrinsicStatus) }
    }

    private fun ExtrinsicStatus.Failure.toSubmissionFailure(): ExtrinsicSubmissionFailure {
        return when (this) {
            is ExtrinsicStatus.FailedToSubmit -> ExtrinsicSubmissionFailure.Submission(exception)
            is ExtrinsicStatus.Invalid -> ExtrinsicSubmissionFailure.TxInvalidation
            is ExtrinsicStatus.Dropped -> ExtrinsicSubmissionFailure.PoolEviction
            is ExtrinsicStatus.Usurped -> ExtrinsicSubmissionFailure.Usurped(by)
        }
    }

    private class FailedAttempt(
        val cause: ExtrinsicSubmissionFailure,
        val terminalStatus: ExtrinsicStatus
    )

    private suspend fun determineExtrinsicOutcome(
        inBlock: ExtrinsicStatus.InBlock,
        chain: Chain
    ): ExtrinsicExecutionResult {
        val outcome = runCancellableCatching {
            val repository = chainEventsRepositoryFactory.create(chain.id)
            val extrinsicWithEvents = repository.getExtrinsicWithEvents(inBlock.extrinsicHash, inBlock.blockHash)
            val runtime = chainRegistry.getRuntime(chain.id)

            requireNotNull(extrinsicWithEvents) {
                "No extrinsic included into expected block"
            }

            extrinsicWithEvents.determineOutcome(runtime)
        }.getOrElse {
            Timber.w(it, "Failed to determine extrinsic outcome")

            ExtrinsicDispatch.Unknown
        }

        return ExtrinsicExecutionResult(
            extrinsicHash = inBlock.extrinsicHash,
            blockHash = inBlock.blockHash,
            outcome = outcome
        )
    }

    private fun ExtrinsicWithEvents.determineOutcome(runtimeSnapshot: RuntimeSnapshot): ExtrinsicDispatch {
        return if (isSuccess()) {
            ExtrinsicDispatch.Ok(events)
        } else {
            val errorEvent = events.findExtrinsicFailureOrThrow()
            val dispatchError = parseErrorEvent(errorEvent, runtimeSnapshot)

            ExtrinsicDispatch.Failed(dispatchError)
        }
    }

    private fun parseErrorEvent(errorEvent: GenericEvent.Instance, runtimeSnapshot: RuntimeSnapshot): DispatchError {
        val dispatchError = errorEvent.arguments.first()

        return bindDispatchError(dispatchError, runtimeSnapshot)
    }

    override suspend fun buildExtrinsic(
        chain: Chain,
        origin: TransactionOrigin,
        options: ExtrinsicService.SubmissionOptions,
        formExtrinsic: FormExtrinsic,
    ): Result<EnrichedSendableExtrinsic> = runCatching {
        buildSubmittableExtrinsic(chain, origin, options, formExtrinsic).extrinsic
    }

    override suspend fun buildExtrinsics(
        chain: Chain,
        options: ExtrinsicService.SubmissionOptions,
        formExtrinsic: FormMultiExtrinsic,
    ): Result<List<EnrichedSendableExtrinsic>> = runCatching {
        buildSubmittableExtrinsicMulti(chain, options, formExtrinsic).map(Submission::extrinsic)
    }

    private fun ExtrinsicService.SubmissionOptions.createExtrinsicFactoryOptions(): ExtrinsicBuilderFactory.Options {
        return ExtrinsicBuilderFactory.Options(
            mortality = mortality,
            tip = tip,
            nonce = nonce,
            metadataHash = metadataHash?.value,
            transactionVersion = transactionVersion,
            specVersion = specVersion,
            batchMode = batchMode,
            extrinsicVersion = extrinsicVersion
        )
    }

    private suspend fun estimateNativeFee(
        chain: Chain,
        extrinsic: SendableExtrinsic,
    ): Fee {
        val baseFee = rpcCalls.getExtrinsicFee(chain.id, extrinsic).partialFee

        val tip = extrinsic.extrinsic.tip().orZero().intoBalance()

        return SimpleFee(
            amount = tip + baseFee,
            asset = chain.utilityAsset
        )
    }

    private suspend fun buildSubmittableExtrinsic(
        chain: Chain,
        origin: TransactionOrigin,
        options: ExtrinsicService.SubmissionOptions,
        formExtrinsic: FormExtrinsic,
    ): Submission {
        val originOrDefault = origin
        val factoryOptions = options.createExtrinsicFactoryOptions()

        val requestedSignerAccountId = originOrDefault.signerSource.accountId(chain)
        val signer = signerProvider.submissionSigner(originOrDefault.signerSource)

        val submittable = extrinsicBuilderFactory.createForSubmission(chain, signer, requestedSignerAccountId, factoryOptions)
        val extrinsicBuilder = submittable.builder
        formExtrinsic(InlineWithRuntime(extrinsicBuilder.runtime), extrinsicBuilder)
        if (originOrDefault.paysFees) options.feePayment.modifyExtrinsic(extrinsicBuilder)
        originOrDefault.applyTo(extrinsicBuilder)
        applyDefaultTransactionExtensions(chain, extrinsicBuilder)
        val extrinsic = extrinsicBuilder.buildExtrinsic()

        return Submission(extrinsic.withMortality(submittable.mortality))
    }

    private suspend fun buildSubmittableExtrinsicMulti(
        chain: Chain,
        options: ExtrinsicService.SubmissionOptions,
        multiBuilder: FormMultiExtrinsic,
    ): Collection<Submission> {
        val factoryOptions = options.createExtrinsicFactoryOptions()
        val perExtrinsicBuilders = StoringMultiExtrinsicBuilder().apply { multiBuilder() }.build()
        val builderSequence = ExtrinsicBuilderSequence(extrinsicBuilderFactory, signerProvider)

        return perExtrinsicBuilders.map {
            val origin = it.origin
            val submittable = builderSequence.next(chain, origin, factoryOptions)
            val extrinsicBuilder = submittable.builder

            it.formExtrinsic(InlineWithRuntime(extrinsicBuilder.runtime), extrinsicBuilder)
            if (origin.paysFees) options.feePayment.modifyExtrinsic(extrinsicBuilder)
            origin.applyTo(extrinsicBuilder)
            applyDefaultTransactionExtensions(chain, extrinsicBuilder)
            val extrinsic = extrinsicBuilder.buildExtrinsic()

            Submission(extrinsic.withMortality(submittable.mortality))
        }
    }

    private fun applyDefaultTransactionExtensions(chain: Chain, extrinsicBuilder: ExtrinsicBuilder) {
        defaultExtensionProviders.forEach { provider ->
            provider.provideFor(chain.id)?.let(extrinsicBuilder::setTransactionExtension)
        }
    }

    private data class Submission(val extrinsic: EnrichedSendableExtrinsic)

    private suspend fun Flow<ExtrinsicStatus>.awaitInBlock(): Result<ExtrinsicStatus.InBlock> {
        return runCancellableCatching {
            val result = first { it.terminal || it is ExtrinsicStatus.InBlock }

            when (result) {
                is ExtrinsicStatus.FailedToSubmit -> throw result.exception
                is ExtrinsicStatus.InBlock -> result
                else -> error("Failed to await tx in block: $result")
            }
        }
    }

    private companion object {
        // Global default: retry a transiently-invalidated extrinsic for a few blocks before giving up.
        const val DEFAULT_RECOVERY_MAX_ATTEMPTS = 3
    }
}
