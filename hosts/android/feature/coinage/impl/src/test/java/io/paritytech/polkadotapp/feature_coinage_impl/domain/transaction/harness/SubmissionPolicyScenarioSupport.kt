package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageInput
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageScheduledTransactionRequest
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionRequest
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.AsyncDurableSubmissionPolicy
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableFailureKind
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxEntry
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.ScheduledDurableTx
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPolicy
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPolicyId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPreparation
import kotlinx.coroutines.awaitCancellation

const val SCRIPTED_POLICY_ID = "scripted"

/** What a scripted policy does with every transaction it is asked to build. */
enum class PolicyBehaviour {
    /** Builds a fresh extrinsic anchored at the finalized head. */
    BUILD,

    /** Waits forever, the way a policy waiting on the chain would, so the entry stays waiting. */
    HOLD,

    GIVE_UP,
}

/**
 * Stands in for a domain's policy: the engine only needs something that builds, waits or gives up, and the
 * scenarios here are about what the engine does with each answer — not about how coinage rebuilds.
 */
class ScriptedSubmissionPolicy(
    private val harness: DurabilityHarness,
    var behaviour: PolicyBehaviour,
) : AsyncDurableSubmissionPolicy {
    override val chainId: String = HARNESS_CHAIN

    /** Every call's transactions, in call order. */
    val prepared = mutableListOf<List<DurableTxId>>()

    override suspend fun canRetry(entry: DurableTxEntry, params: DataByteArray, failure: DurableFailureKind): Boolean = true

    override suspend fun prepareSubmission(
        transactions: List<ScheduledDurableTx>,
    ): Result<Map<DurableTxId, SubmissionPreparation>> {
        prepared += transactions.map { it.id }

        return when (behaviour) {
            PolicyBehaviour.BUILD -> Result.success(
                transactions.associate { it.id to SubmissionPreparation.Ready(harness.extrinsicAnchoredAtFinalizedHead()) }
            )

            PolicyBehaviour.HOLD -> awaitCancellation()

            PolicyBehaviour.GIVE_UP -> Result.success(transactions.associate { it.id to SubmissionPreparation.GiveUp })
        }
    }
}

/** Installs a scripted policy under [SCRIPTED_POLICY_ID] and starts the executor, as a launch would. */
fun DurabilityHarness.givenSubmissionPolicy(behaviour: PolicyBehaviour): ScriptedSubmissionPolicy {
    val policy = ScriptedSubmissionPolicy(this, behaviour)
    submissionPolicies = mapOf(SCRIPTED_POLICY_ID to policy)
    startExecutor()

    return policy
}

fun scriptedPolicy() = SubmissionPolicy(SubmissionPolicyId(SCRIPTED_POLICY_ID), byteArrayOf().toDataByteArray())

/** One coin in, [outputCoins] out, registered with the scripted policy so a proven failure is built again. */
suspend fun DurabilityHarness.registerRetriable(
    inputCoin: Int,
    vararg outputCoins: Int,
    groupId: CoinageOperationGroupId = CoinageOperationGroupId("retriable"),
): CoinageTransactionId = service.submitTransactions(
    transactions = listOf(
        CoinageTransactionRequest(
            extrinsic = extrinsicAnchoredAtFinalizedHead(),
            inputs = listOf(CoinageInput.Coin.Own(testKey(inputCoin))),
            outputs = outputCoins.map { OwnAsset.Coin(testKey(it)) },
            policy = scriptedPolicy(),
        )
    ),
    groupId = groupId,
).getOrThrow().single()

/** The same shape, registered without an extrinsic for the scripted policy to build. */
suspend fun DurabilityHarness.scheduleRetriable(
    inputCoin: Int,
    vararg outputCoins: Int,
    groupId: CoinageOperationGroupId = CoinageOperationGroupId("scheduled"),
): CoinageTransactionId = service.scheduleTransactions(
    transactions = listOf(
        CoinageScheduledTransactionRequest(
            policy = scriptedPolicy(),
            inputs = listOf(CoinageInput.Coin.Own(testKey(inputCoin))),
            outputs = outputCoins.map { OwnAsset.Coin(testKey(it)) },
        )
    ),
    groupId = groupId,
).getOrThrow().single()

suspend fun DurabilityHarness.txHashOf(id: CoinageTransactionId): String? = repository.getEntry(id).getOrThrow()?.txHash
