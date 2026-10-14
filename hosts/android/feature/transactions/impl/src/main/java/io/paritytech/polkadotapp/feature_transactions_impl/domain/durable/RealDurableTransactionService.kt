package io.paritytech.polkadotapp.feature_transactions_impl.domain.durable

import io.paritytech.polkadotapp.feature_transactions.api.data.EnrichedSendableExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableSubmission
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTransactionService
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxState
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.OperationGroupId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.RegistrationScope
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPolicy
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TxDomainId
import io.paritytech.polkadotapp.feature_transactions_impl.data.durable.DurableTxRegistration
import io.paritytech.polkadotapp.feature_transactions_impl.data.durable.DurableTxRepository
import io.paritytech.polkadotapp.feature_transactions_impl.data.durable.DurableTxSchedule
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Orchestrates registration, submission, asynchronous building and the recovery pass, and exposes the queries.
 *
 * It holds no durable state of its own: everything it knows lives in the ledger and everything volatile
 * lives in the collaborators, so process death loses exactly the volatile half — which is what makes
 * [close] a faithful crash in tests.
 */
@Singleton
class RealDurableTransactionService @Inject constructor(
    private val repository: DurableTxRepository,
    private val submissionOwned: SubmissionOwnedTransactions,
    private val launcher: DurableSubmissionLauncher,
    private val executor: DurableSubmissionExecutor,
    private val recoveryScheduler: DurableRecoveryScheduler,
) : DurableTransactionService {
    override suspend fun submit(
        domain: TxDomainId,
        extrinsic: EnrichedSendableExtrinsic,
        groupId: OperationGroupId?,
        onRegister: suspend RegistrationScope.(DurableTxId) -> Unit,
    ): Result<DurableTxId> {
        val registration = runCatching { extrinsic.toRegistration(domain, groupId, policy = null) }
            .getOrElse { return Result.failure(it) }

        // Registration commits before submission, so there is never an extrinsic in flight without a record
        // holding what it consumes. The reverse order would leave a window where a crash loses the lock for
        // bytes already on the wire.
        return repository.register(registration) { id ->
            onRegister(id)
            // Taken inside the same transaction, so a committed row always has an owner and a pass can
            // never reach it before the watcher does.
            submissionOwned.acquire(id, registration.attempt.txHash)
        }.onSuccess { id ->
            logRegistered(domain, id, registration)
            launcher.watch(id, extrinsic)
        }.onFailure { durabilityLogW("submit failed domain=${domain.value} error=$it") }
    }

    override suspend fun submitAll(
        domain: TxDomainId,
        submissions: List<DurableSubmission>,
        groupId: OperationGroupId,
        onRegister: suspend RegistrationScope.(List<DurableTxId>) -> Unit,
    ): Result<List<DurableTxId>> {
        val registrations = runCatching {
            submissions.map { it.extrinsic.toRegistration(domain, groupId, it.policy) }
        }.getOrElse { return Result.failure(it) }

        return repository.registerAll(registrations) { ids ->
            onRegister(ids)
            ids.zip(registrations).forEach { (id, registration) -> submissionOwned.acquire(id, registration.attempt.txHash) }
        }.onSuccess { ids ->
            ids.zip(registrations).forEach { (id, registration) -> logRegistered(domain, id, registration) }
            ids.zip(submissions).forEach { (id, submission) -> launcher.watch(id, submission.extrinsic) }
        }.onFailure { durabilityLogW("submitAll failed domain=${domain.value} group=${groupId.value} error=$it") }
    }

    override suspend fun schedule(
        domain: TxDomainId,
        groupId: OperationGroupId,
        policies: List<SubmissionPolicy>,
        onRegister: suspend RegistrationScope.(List<DurableTxId>) -> Unit,
    ): Result<List<DurableTxId>> {
        val schedules = policies.map { DurableTxSchedule(domain, groupId, it) }

        return repository.schedule(schedules, onRegister)
            .onSuccess { ids ->
                durabilityLogI(
                    "entries-scheduled domain=${domain.value} group=${groupId.value} " +
                        "entries=${ids.map { it.value }} policies=${policies.map { it.id.value }.distinct()}"
                )

                // Nothing is built until the executor reads these rows, so this is safe while an enclosing
                // transaction has not committed them yet.
                startRecovery()
            }
            .onFailure { durabilityLogW("schedule failed domain=${domain.value} group=${groupId.value} error=$it") }
    }

    override fun startRecovery() {
        durabilityLogD("start-recovery")

        executor.ensureStarted()
        recoveryScheduler.ensureRunning()
    }

    override suspend fun getStatus(id: DurableTxId): Result<DurableTxStatus> =
        repository.getStatus(id).mapCatching { it ?: error("No durable transaction ${id.value}") }

    override fun subscribeStatus(id: DurableTxId): Flow<DurableTxStatus> = repository.subscribeStatus(id)

    override suspend fun getGroupStates(
        domain: TxDomainId,
        groupId: OperationGroupId,
    ): Result<List<DurableTxState>> = repository.getGroupStates(domain, groupId)

    override fun subscribeGroupStates(
        domain: TxDomainId,
        groupId: OperationGroupId,
    ): Flow<List<DurableTxState>> = repository.subscribeGroupStates(domain, groupId)

    /** Cancels every watch, build and pass this instance owns. */
    fun close() {
        launcher.close()
        executor.close()
    }

    private fun logRegistered(domain: TxDomainId, id: DurableTxId, registration: DurableTxRegistration) {
        val attempt = registration.attempt

        durabilityLogI(
            "entry-registered ${durabilityLogId(domain, id, attempt.txHash, registration.groupId)} " +
                "checkpoint=${attempt.checkpoint.blockNumber} mortality=${attempt.mortalityBlocks} " +
                "window=${attempt.checkpoint.blockNumber}..${attempt.checkpoint.blockNumber + attempt.mortalityBlocks} " +
                "policy=${registration.policy?.id?.value}"
        )
    }

    private fun EnrichedSendableExtrinsic.toRegistration(
        domain: TxDomainId,
        groupId: OperationGroupId?,
        policy: SubmissionPolicy?,
    ) = DurableTxRegistration(
        domainId = domain,
        groupId = groupId,
        attempt = toAttempt(),
        policy = policy,
    )
}
