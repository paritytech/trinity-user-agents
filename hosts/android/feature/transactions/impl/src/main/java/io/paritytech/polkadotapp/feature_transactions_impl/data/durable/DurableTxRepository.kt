package io.paritytech.polkadotapp.feature_transactions_impl.data.durable

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.database.dao.DurableTxDao
import io.paritytech.polkadotapp.database.model.BlockRefLocal
import io.paritytech.polkadotapp.database.model.DurableTxLocal
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.CheckpointBlock
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxEntry
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxState
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.OperationGroupId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.RegistrationScope
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.ScheduledDurableTx
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPolicy
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPolicyId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TxDomainId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.Verdict
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionHash
import io.paritytech.polkadotapp.feature_transactions_impl.domain.durable.durabilityLogI
import io.paritytech.polkadotapp.feature_transactions_impl.domain.durable.durabilityLogW
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.map
import javax.inject.Inject

/** One attempt at a transaction: the bytes' hash and the window they can land in. */
class DurableTxAttempt(
    val txHash: TransactionHash,
    val checkpoint: CheckpointBlock,
    val mortalityBlocks: Long,
)

/** What registration needs to write one transaction row. */
class DurableTxRegistration(
    val domainId: TxDomainId,
    val groupId: OperationGroupId?,
    val attempt: DurableTxAttempt,
    val policy: SubmissionPolicy?,
)

/** What scheduling needs to write one transaction row that has not been built yet. */
class DurableTxSchedule(
    val domainId: TxDomainId,
    val groupId: OperationGroupId,
    val policy: SubmissionPolicy,
)

/**
 * The engine's durable ledger: one row per transaction it has taken responsibility for.
 *
 * It stores nothing domain-shaped. A domain writes its own rows through [RegistrationScope], inside the
 * same database transaction, so the two commit together.
 */
interface DurableTxRepository {
    /**
     * Inserts [registration], runs [onRegister] in the same transaction, and commits both.
     *
     * Throwing from [onRegister] rolls the whole thing back, which is how a domain rejects a registration
     * its own invariants forbid.
     */
    suspend fun register(
        registration: DurableTxRegistration,
        onRegister: suspend RegistrationScope.(DurableTxId) -> Unit,
    ): Result<DurableTxId>

    /** All of [registrations] or none: half a group in the ledger means the opposite of what it looks like. */
    suspend fun registerAll(
        registrations: List<DurableTxRegistration>,
        onRegister: suspend RegistrationScope.(List<DurableTxId>) -> Unit,
    ): Result<List<DurableTxId>>

    /** Inserts rows waiting to be built, runs [onRegister] in the same transaction, and commits both. */
    suspend fun schedule(
        schedules: List<DurableTxSchedule>,
        onRegister: suspend RegistrationScope.(List<DurableTxId>) -> Unit,
    ): Result<List<DurableTxId>>

    /** Null for a transaction that has never been built. */
    suspend fun getEntry(id: DurableTxId): Result<DurableTxEntry?>

    /** Every transaction that carries an attempt; one waiting to be built has nothing a pass could decide. */
    suspend fun getAllEntries(domainId: TxDomainId): Result<List<DurableTxEntry>>

    suspend fun getSubmissionPolicy(id: DurableTxId): Result<SubmissionPolicy?>

    /** Emits the transactions waiting to be built whenever the ledger changes. */
    fun subscribePendingSubmissions(): Flow<List<ScheduledDurableTx>>

    suspend fun getPendingSubmissions(
        policyId: SubmissionPolicyId,
        groupId: OperationGroupId?,
    ): Result<List<ScheduledDurableTx>>

    /** Replaces the attempt of a transaction waiting to be built and makes it pending. Returns whether it wrote. */
    suspend fun startAttempt(id: DurableTxId, attempt: DurableTxAttempt): Result<Boolean>

    /** Fails a transaction waiting to be built. Returns whether it wrote. */
    suspend fun abandonSubmission(id: DurableTxId): Result<Boolean>

    suspend fun getStatus(id: DurableTxId): Result<DurableTxStatus?>

    fun subscribeStatus(id: DurableTxId): Flow<DurableTxStatus>

    /**
     * Writes only while the transaction still reads [observed] and its attempt is still [observedTxHash].
     * Returns whether it wrote.
     */
    suspend fun compareAndSetStatus(
        id: DurableTxId,
        observed: DurableTxStatus,
        observedTxHash: TransactionHash,
        verdict: Verdict,
    ): Result<Boolean>

    suspend fun hasLiveTransactions(): Result<Boolean>

    /** Domains with something a pass could still decide. */
    suspend fun liveDomains(): Result<List<TxDomainId>>

    suspend fun getGroupStates(domainId: TxDomainId, groupId: OperationGroupId): Result<List<DurableTxState>>

    fun subscribeGroupStates(domainId: TxDomainId, groupId: OperationGroupId): Flow<List<DurableTxState>>
}

class RealDurableTxRepository @Inject constructor(
    private val dao: DurableTxDao,
) : DurableTxRepository {
    override suspend fun register(
        registration: DurableTxRegistration,
        onRegister: suspend RegistrationScope.(DurableTxId) -> Unit,
    ): Result<DurableTxId> = runCatching {
        var id = DurableTxId(DurableTxLocal.UNSAVED_ID)

        dao.withTransaction {
            id = DurableTxId(dao.insert(registration.toLocal()))
            RegistrationScope.onRegister(id)
        }

        id
    }

    override suspend fun registerAll(
        registrations: List<DurableTxRegistration>,
        onRegister: suspend RegistrationScope.(List<DurableTxId>) -> Unit,
    ): Result<List<DurableTxId>> = runCatching {
        val ids = mutableListOf<DurableTxId>()

        dao.withTransaction {
            registrations.mapTo(ids) { DurableTxId(dao.insert(it.toLocal())) }
            RegistrationScope.onRegister(ids)
        }

        ids
    }

    override suspend fun schedule(
        schedules: List<DurableTxSchedule>,
        onRegister: suspend RegistrationScope.(List<DurableTxId>) -> Unit,
    ): Result<List<DurableTxId>> = runCatching {
        val ids = mutableListOf<DurableTxId>()

        dao.withTransaction {
            schedules.mapTo(ids) { DurableTxId(dao.insert(it.toLocal())) }
            RegistrationScope.onRegister(ids)
        }

        ids
    }

    override suspend fun getEntry(id: DurableTxId): Result<DurableTxEntry?> =
        runCatching { dao.get(id.value)?.toEntryOrNull() }

    override suspend fun getAllEntries(domainId: TxDomainId): Result<List<DurableTxEntry>> =
        runCatching { dao.getAllSubmitted(domainId.value).mapNotNull { it.toEntryOrNull() } }

    override suspend fun getSubmissionPolicy(id: DurableTxId): Result<SubmissionPolicy?> =
        runCatching { dao.get(id.value)?.toPolicyOrNull() }

    override fun subscribePendingSubmissions(): Flow<List<ScheduledDurableTx>> =
        dao.subscribePendingSubmissions().map { rows -> rows.mapNotNull { it.toScheduledOrNull() } }

    override suspend fun getPendingSubmissions(
        policyId: SubmissionPolicyId,
        groupId: OperationGroupId?,
    ): Result<List<ScheduledDurableTx>> = runCatching {
        dao.getPendingSubmissions(policyId.value, groupId?.value).mapNotNull { it.toScheduledOrNull() }
    }

    override suspend fun startAttempt(id: DurableTxId, attempt: DurableTxAttempt): Result<Boolean> = runCatching {
        val written = dao.startAttempt(
            id = id.value,
            txHash = attempt.txHash,
            checkpointBlockNumber = attempt.checkpoint.blockNumber,
            checkpointBlockHash = attempt.checkpoint.blockHash,
            mortalityBlocks = attempt.mortalityBlocks,
        )

        if (written > 0) {
            durabilityLogI("entry=${id.value} attempt-started hash=${attempt.txHash} checkpoint=${attempt.checkpoint.blockNumber}")
        } else {
            durabilityLogW("entry=${id.value} attempt-declined reason=not-pending-submission")
        }

        written > 0
    }

    override suspend fun abandonSubmission(id: DurableTxId): Result<Boolean> = runCatching {
        val written = dao.abandonSubmission(id.value)

        if (written > 0) {
            durabilityLogI("entry=${id.value} submission-abandoned")
        } else {
            durabilityLogW("entry=${id.value} submission-abandon-declined reason=not-pending-submission")
        }

        written > 0
    }

    override suspend fun getStatus(id: DurableTxId): Result<DurableTxStatus?> =
        runCatching { dao.getStatus(id.value)?.toDomain() }

    override fun subscribeStatus(id: DurableTxId): Flow<DurableTxStatus> =
        dao.subscribeStatus(id.value).filterNotNull().map { it.toDomain() }

    override suspend fun compareAndSetStatus(
        id: DurableTxId,
        observed: DurableTxStatus,
        observedTxHash: TransactionHash,
        verdict: Verdict,
    ): Result<Boolean> = runCatching {
        val written = dao.compareAndSetStatus(
            id = id.value,
            expected = observed.toLocal(),
            expectedTxHash = observedTxHash,
            status = verdict.status.toLocal(),
            successDetectedBlockNumber = verdict.successDetectedAt?.blockNumber,
            successDetectedBlockHash = verdict.successDetectedAt?.blockHash,
        )

        val record = verdict.successDetectedAt?.blockNumber?.toString() ?: "none"

        if (written > 0) {
            durabilityLogI("entry=${id.value} cas-written from=$observed to=${verdict.status} record=$record")
        } else {
            durabilityLogW("entry=${id.value} cas-declined observed=$observed to=${verdict.status} record=$record")
        }

        written > 0
    }

    override suspend fun hasLiveTransactions(): Result<Boolean> = runCatching { dao.hasLiveTransactions() }

    override suspend fun liveDomains(): Result<List<TxDomainId>> =
        runCatching { dao.liveDomains().map(::TxDomainId) }

    override suspend fun getGroupStates(
        domainId: TxDomainId,
        groupId: OperationGroupId,
    ): Result<List<DurableTxState>> = runCatching {
        dao.getGroup(domainId.value, groupId.value).map { it.toState() }
    }

    override fun subscribeGroupStates(
        domainId: TxDomainId,
        groupId: OperationGroupId,
    ): Flow<List<DurableTxState>> =
        dao.subscribeGroup(domainId.value, groupId.value).map { rows -> rows.map { it.toState() } }

    private fun DurableTxRegistration.toLocal() = DurableTxLocal(
        id = DurableTxLocal.UNSAVED_ID,
        domainId = domainId.value,
        operationGroupId = groupId?.value,
        txHash = attempt.txHash,
        checkpoint = BlockRefLocal(attempt.checkpoint.blockNumber, attempt.checkpoint.blockHash),
        mortalityBlocks = attempt.mortalityBlocks,
        successDetectedAt = null,
        status = DurableTxLocal.Status.PENDING,
        submissionPolicyId = policy?.id?.value,
        submissionPolicyParams = policy?.params?.value,
    )

    private fun DurableTxSchedule.toLocal() = DurableTxLocal(
        id = DurableTxLocal.UNSAVED_ID,
        domainId = domainId.value,
        operationGroupId = groupId.value,
        txHash = null,
        checkpoint = null,
        mortalityBlocks = null,
        successDetectedAt = null,
        status = DurableTxLocal.Status.PENDING_SUBMISSION,
        submissionPolicyId = policy.id.value,
        submissionPolicyParams = policy.params.value,
    )

    private fun DurableTxLocal.toEntryOrNull(): DurableTxEntry? {
        val hash = txHash ?: return null
        val anchor = checkpoint ?: return null
        val mortality = mortalityBlocks ?: return null

        return DurableTxEntry(
            id = DurableTxId(id),
            domainId = TxDomainId(domainId),
            groupId = operationGroupId?.let(::OperationGroupId),
            txHash = hash,
            checkpoint = CheckpointBlock(anchor.blockNumber, anchor.blockHash),
            mortalityBlocks = mortality,
            status = status.toDomain(),
            successDetectedAt = successDetectedAt?.let { CheckpointBlock(it.blockNumber, it.blockHash) },
        )
    }

    private fun DurableTxLocal.toPolicyOrNull(): SubmissionPolicy? {
        val policyId = submissionPolicyId ?: return null
        val params = submissionPolicyParams ?: return null

        return SubmissionPolicy(SubmissionPolicyId(policyId), params.toDataByteArray())
    }

    private fun DurableTxLocal.toScheduledOrNull(): ScheduledDurableTx? {
        val policy = toPolicyOrNull() ?: run {
            durabilityLogW("entry=$id pending-submission-without-policy")

            return null
        }

        return ScheduledDurableTx(
            id = DurableTxId(id),
            domainId = TxDomainId(domainId),
            groupId = operationGroupId?.let(::OperationGroupId),
            policy = policy,
        )
    }

    private fun DurableTxLocal.toState() = DurableTxState(DurableTxId(id), status.toDomain())

    private fun DurableTxLocal.Status.toDomain() = when (this) {
        DurableTxLocal.Status.PENDING -> DurableTxStatus.PENDING
        DurableTxLocal.Status.PENDING_SUCCESS -> DurableTxStatus.PENDING_SUCCESS
        DurableTxLocal.Status.FINALIZED_SUCCESS -> DurableTxStatus.FINALIZED_SUCCESS
        DurableTxLocal.Status.FAILURE -> DurableTxStatus.FAILURE
        DurableTxLocal.Status.PENDING_SUBMISSION -> DurableTxStatus.PENDING_SUBMISSION
    }

    private fun DurableTxStatus.toLocal() = when (this) {
        DurableTxStatus.PENDING -> DurableTxLocal.Status.PENDING
        DurableTxStatus.PENDING_SUCCESS -> DurableTxLocal.Status.PENDING_SUCCESS
        DurableTxStatus.FINALIZED_SUCCESS -> DurableTxLocal.Status.FINALIZED_SUCCESS
        DurableTxStatus.FAILURE -> DurableTxLocal.Status.FAILURE
        DurableTxStatus.PENDING_SUBMISSION -> DurableTxLocal.Status.PENDING_SUBMISSION
    }
}
