package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.EntryAssets
import io.paritytech.polkadotapp.feature_transactions.api.data.EnrichedSendableExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.ScheduledDurableTx
import kotlinx.coroutines.flow.Flow
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

/**
 * How long a transaction's rebuilds go on: until [deadline] has passed with its inputs gone, and past a failed
 * attempt only when [retriesFailures].
 */
@OptIn(ExperimentalTime::class)
data class RebuildTerms(
    val deadline: Instant,
    val retriesFailures: Boolean,
)

/**
 * What one kind of coinage transaction contributes to [InputGatedSubmissionPolicy]: how it is read back from the
 * ledger, which on-chain assets it waits for, and how it is built. When to wait, build, give up or retry is the
 * policy's alone.
 *
 * [T] is one transaction resolved for building; [K] identifies one of its inputs.
 */
interface CoinageRebuild<T : Any, K> {
    /** Null when [params] cannot be read, which makes the transaction unbuildable. */
    fun termsOf(params: DataByteArray): RebuildTerms?

    /** Transactions that cannot be resolved are left out, and given up on: nothing recorded will change. */
    suspend fun resolve(
        transactions: List<ScheduledDurableTx>,
        assets: Map<DurableTxId, EntryAssets>,
    ): Map<DurableTxId, T>

    fun inputsOf(transaction: T): Set<K>

    /** Which of [inputs] are present, on every look that could be taken. */
    suspend fun presence(inputs: Set<K>): Flow<Set<K>>

    /** One extrinsic per transaction, in the order of [transactions]. */
    suspend fun build(transactions: List<T>): Result<List<EnrichedSendableExtrinsic>>
}
