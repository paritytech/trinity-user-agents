package io.paritytech.polkadotapp.feature_transactions.api.domain.durable

import io.paritytech.polkadotapp.feature_transactions.api.data.EnrichedSendableExtrinsic
import kotlinx.coroutines.flow.Flow

/**
 * Marker for the engine's open write transaction.
 *
 * A domain writes its own rows through its own DAO from inside a block receiving this, so those rows and
 * the ledger row commit together. Throwing from that block rolls both back.
 */
object RegistrationScope

/**
 * Owns one fact: the status of every submitted transaction, for every domain.
 *
 * Whatever a domain locks for a transaction it keeps itself, written through [RegistrationScope]. This
 * service never reads chain state of its own beyond block bodies and the heads — anything domain-shaped
 * reaches it only through that domain's [TxCompletionOracle].
 */
interface DurableTransactionService {
    /**
     * Registers an already-signed transaction, then starts submission. Returns once committed, which is
     * before the bytes reach the wire: no extrinsic is ever in flight without a record holding what it
     * consumes.
     *
     * The checkpoint is the anchor [extrinsic] was signed over and the mortality comes from its era.
     */
    suspend fun submit(
        domain: TxDomainId,
        extrinsic: EnrichedSendableExtrinsic,
        groupId: OperationGroupId?,
        onRegister: suspend RegistrationScope.(DurableTxId) -> Unit,
    ): Result<DurableTxId>

    /**
     * Registers several transactions as one operation: either all of them are recorded or none is.
     *
     * A caller that reads a group back as a single outcome needs this — half a group in the ledger looks
     * exactly like a group whose other half failed on chain, and the two mean opposite things.
     *
     * A submission carrying a policy is built again by it once an attempt is proven unable to land, rather
     * than failing.
     */
    suspend fun submitAll(
        domain: TxDomainId,
        submissions: List<DurableSubmission>,
        groupId: OperationGroupId,
        onRegister: suspend RegistrationScope.(List<DurableTxId>) -> Unit,
    ): Result<List<DurableTxId>>

    /**
     * Registers transactions that have not been built yet, one per policy, as one operation. Each is built
     * and submitted by its policy afterwards, and built again as [submitAll] describes.
     *
     * What they will consume is locked from the moment this commits. Safe to call inside an enclosing database
     * transaction: building starts only once the rows are committed.
     */
    suspend fun schedule(
        domain: TxDomainId,
        groupId: OperationGroupId,
        policies: List<SubmissionPolicy>,
        onRegister: suspend RegistrationScope.(List<DurableTxId>) -> Unit,
    ): Result<List<DurableTxId>>

    /**
     * Ensures recovery is running: transactions waiting to be built are built, and one pass runs per newly seen
     * head, until nothing is left undecided.
     *
     * Fire-and-forget and idempotent — a caller states that transactions may need deciding, not that a
     * pass should happen now.
     */
    fun startRecovery()

    suspend fun getStatus(id: DurableTxId): Result<DurableTxStatus>

    fun subscribeStatus(id: DurableTxId): Flow<DurableTxStatus>

    /** In registration order. Empty when nothing was ever registered under [groupId]. */
    suspend fun getGroupStates(domain: TxDomainId, groupId: OperationGroupId): Result<List<DurableTxState>>

    /** In registration order. */
    fun subscribeGroupStates(domain: TxDomainId, groupId: OperationGroupId): Flow<List<DurableTxState>>
}

/** Raised when a registration would break an invariant the engine enforces for every domain. */
sealed class DurableTxRegistrationError(message: String) : Exception(message) {
    data object NotMortal : DurableTxRegistrationError("Only mortal extrinsics can be made durable") {
        private fun readResolve(): Any = NotMortal
    }

    data object MissingEraAnchor : DurableTxRegistrationError("Extrinsic reports no era anchor block") {
        private fun readResolve(): Any = MissingEraAnchor
    }
}
