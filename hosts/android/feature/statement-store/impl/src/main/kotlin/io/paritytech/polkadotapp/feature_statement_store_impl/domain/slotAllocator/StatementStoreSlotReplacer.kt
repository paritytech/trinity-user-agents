@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator

import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.repository.ChainStateRepository
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.foldResult
import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.common.utils.progressStallReport.markRegion
import io.paritytech.polkadotapp.common.utils.requireNotNull
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.AccountDerivationUseCase
import io.paritytech.polkadotapp.feature_chain_resources_api.data.api.replaceStatementStoreAccount
import io.paritytech.polkadotapp.feature_chain_resources_api.data.api.resourcesCalls
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.CollectionSlot
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlot
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlotAllocationError
import io.paritytech.polkadotapp.feature_statement_store_impl.data.repository.StatementStoreSlotAllocationRepository
import io.paritytech.polkadotapp.feature_statement_store_impl.data.signer.origins.StatementStoreOrigins
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.data.FormExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.data.flattenExecutionFailure
import io.paritytech.polkadotapp.feature_transactions.api.data.validation.ExtrinsicValidator
import io.paritytech.polkadotapp.feature_transactions.api.data.validation.InvalidTransaction
import io.paritytech.polkadotapp.feature_transactions.api.data.validation.TransactionValidity
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import timber.log.Timber
import javax.inject.Inject
import kotlin.time.ExperimentalTime
import io.paritytech.polkadotapp.common.R as RCommon

private const val REPLACEMENT_TOO_EARLY_VALIDITY_CODE: UByte = 228u

class StatementStoreSlotReplacer @Inject constructor(
    private val extrinsicService: ExtrinsicService,
    private val extrinsicValidator: ExtrinsicValidator,
    private val chainStateRepository: ChainStateRepository,
    private val statementStoreOrigins: StatementStoreOrigins,
    private val accountDerivationUseCase: AccountDerivationUseCase,
    private val allocationRepository: StatementStoreSlotAllocationRepository,
    private val slotLoader: StatementStoreSlotLoader,
) {
    context(diagnostics: StalenessReportCollector)
    suspend fun replaceFirstAvailable(
        context: AllocateContext,
        candidates: List<CollectionSlot<StatementStoreSlot.Taken>>,
        newAccount: AccountId,
    ): Result<CollectionSlot<StatementStoreSlot.Taken>> {
        return candidatesSignedOnThisDevice(context, candidates).flatMap { signable ->
            Timber.i("allocate: ${signable.size} of ${candidates.size} eviction candidates are held by keys on this device")
            if (signable.isEmpty()) {
                Result.failure(StatementStoreSlotAllocationError.NoAllocationAvailable(IllegalStateException("No evictable slot is held by a key on this device")))
            } else {
                replaceFirstAllowed(context, signable, newAccount)
            }
        }
    }

    private suspend fun candidatesSignedOnThisDevice(
        context: AllocateContext,
        candidates: List<CollectionSlot<StatementStoreSlot.Taken>>,
    ): Result<List<CandidateWithHolderOrigin>> {
        return runCancellableCatching {
            allocationRepository.signerDerivationPathsFor(context.chain.id, candidates.mapToSet { it.slot.accountId })
        }.flatMap { signerPaths ->
            candidates.foldResult(emptyList()) { signable, candidate ->
                val signerPath = signerPaths[candidate.slot.accountId] ?: return@foldResult Result.success(signable)

                holderKeypair(signerPath, candidate.slot.accountId).map { keypair ->
                    if (keypair == null) {
                        signable
                    } else {
                        signable + CandidateWithHolderOrigin(candidate, statementStoreOrigins.asResourcesSlotHolder(keypair))
                    }
                }
            }
        }
    }

    context(diagnostics: StalenessReportCollector)
    private suspend fun replaceFirstAllowed(
        context: AllocateContext,
        signable: List<CandidateWithHolderOrigin>,
        newAccount: AccountId,
    ): Result<CollectionSlot<StatementStoreSlot.Taken>> {
        return signable.foldResult<_, CollectionSlot<StatementStoreSlot.Taken>?>(null) { replaced, signableCandidate ->
            if (replaced != null) return@foldResult Result.success(replaced)

            val candidate = signableCandidate.candidate
            replaceAsHolder(context, candidate, signableCandidate.holderOrigin, newAccount).map { outcome ->
                Timber.i("allocate: replacing seq=${candidate.slot.seq} in ${candidate.collection}: $outcome")
                candidate.takeIf { outcome == ReplacementOutcome.REPLACED }
            }
        }.requireNotNull { EvictionNotYetAllowedException("No evictable slot this device can sign for may be replaced yet") }
    }

    private suspend fun holderKeypair(derivationPath: String, holder: AccountId): Result<Sr25519Keypair?> {
        return accountDerivationUseCase.deriveKeypair(derivationPath).map { keypair ->
            (keypair as? Sr25519Keypair)?.takeIf { it.publicKey.toDataByteArray() == holder }
        }
    }

    context(diagnostics: StalenessReportCollector)
    private suspend fun replaceAsHolder(
        context: AllocateContext,
        candidate: CollectionSlot<StatementStoreSlot.Taken>,
        holderOrigin: TransactionOrigin,
        newAccount: AccountId,
    ): Result<ReplacementOutcome> {
        return slotLoader.slotAlias(candidate.collection, context.period, candidate.slot.seq).flatMap { alias ->
            val replaceCall: FormExtrinsic = {
                resourcesCalls.replaceStatementStoreAccount(
                    period = context.period,
                    alias = alias.value.toDataByteArray(),
                    newAccount = newAccount,
                    since = candidate.slot.since.epochSeconds.toULong(),
                )
            }

            when (val dryRun = dryRun(context.chain, holderOrigin, replaceCall)) {
                ReplacementDryRun.Valid,
                ReplacementDryRun.Inconclusive -> submit(context.chain, holderOrigin, replaceCall).map { ReplacementOutcome.REPLACED }
                ReplacementDryRun.NotYetAllowed -> Result.success(ReplacementOutcome.NOT_YET_ALLOWED)
                is ReplacementDryRun.Rejected -> Result.failure(SlotReplacementRejectedException(dryRun.reason))
            }
        }
    }

    context(diagnostics: StalenessReportCollector)
    private suspend fun dryRun(chain: Chain, holderOrigin: TransactionOrigin, replaceCall: FormExtrinsic): ReplacementDryRun =
        diagnostics.markRegion(RCommon.string.stall_reading_chain_state) {
            extrinsicService.buildExtrinsic(chain, holderOrigin, ExtrinsicService.SubmissionOptions(), replaceCall)
                .flatMap { extrinsic ->
                    runCancellableCatching { chainStateRepository.currentBlockHash(chain.id) }.flatMap { blockHash ->
                        extrinsicValidator.validate(chain.id, extrinsic.bytesWithoutLength.toDataByteArray(), blockHash)
                    }
                }
                .fold(
                    onSuccess = { validity -> validity.toReplacementDryRun() },
                    onFailure = { error ->
                        Timber.w(error, "allocate: slot replacement dry run failed; submitting without it")
                        ReplacementDryRun.Inconclusive
                    },
                )
        }

    context(diagnostics: StalenessReportCollector)
    private suspend fun submit(chain: Chain, holderOrigin: TransactionOrigin, replaceCall: FormExtrinsic): Result<Unit> =
        diagnostics.markRegion(RCommon.string.stall_submitting_transaction) {
            extrinsicService.submitExtrinsicAndAwaitExecution(chain, holderOrigin, formExtrinsic = replaceCall)
                .flattenExecutionFailure()
                .coerceToUnit()
        }
}

private class CandidateWithHolderOrigin(
    val candidate: CollectionSlot<StatementStoreSlot.Taken>,
    val holderOrigin: TransactionOrigin,
)

private enum class ReplacementOutcome {
    REPLACED,
    NOT_YET_ALLOWED,
}

private sealed interface ReplacementDryRun {
    data object Valid : ReplacementDryRun

    data object Inconclusive : ReplacementDryRun

    data object NotYetAllowed : ReplacementDryRun

    class Rejected(val reason: InvalidTransaction) : ReplacementDryRun
}

private fun TransactionValidity.toReplacementDryRun(): ReplacementDryRun = when (this) {
    TransactionValidity.Valid -> ReplacementDryRun.Valid
    is TransactionValidity.Unknown -> ReplacementDryRun.Inconclusive
    is TransactionValidity.Invalid -> if (reason.isReplacementNotYetAllowed()) {
        ReplacementDryRun.NotYetAllowed
    } else {
        ReplacementDryRun.Rejected(reason)
    }
}

private fun InvalidTransaction.isReplacementNotYetAllowed(): Boolean {
    return this is InvalidTransaction.Payment || this == InvalidTransaction.Custom(REPLACEMENT_TOO_EARLY_VALIDITY_CODE)
}
