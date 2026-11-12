package io.paritytech.polkadotapp.feature_statement_store_impl.domain.notificationAllocator

import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_statement_store_impl.data.repository.NotificationSlotRepository
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.CheckpointBlock
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxEntry
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.MonotoneEffectOracle
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton

// Sound as a monotone oracle: callers claim for a fresh account per period, nothing but this claim grants it an
// allowance, and the allowance is only revoked a full grace window after the period — long after a verdict.
@Singleton
class NotificationAllowanceOracle @Inject constructor(
    private val knownChains: KnownChains,
    private val notificationSlotRepository: NotificationSlotRepository,
) : MonotoneEffectOracle() {
    override val chainId: ChainId
        get() = knownChains.people

    override suspend fun effectsAt(transactions: List<DurableTxEntry>, at: CheckpointBlock): Map<DurableTxId, Boolean> {
        val targetByClaim = targetsOf(transactions)
        val targets = targetByClaim.values.distinct()
        val granted = allowancesAt(targets, at) ?: return emptyMap()
        Timber.d("notificationAllowanceOracle: ${granted.count { it.value }}/${targets.size} targets hold an allowance at block ${at.blockNumber}")

        return targetByClaim
            .mapNotNull { (claim, target) -> granted[target]?.let { claim to it } }
            .toMap()
    }

    private fun targetsOf(transactions: List<DurableTxEntry>): Map<DurableTxId, AccountId> {
        return transactions.mapNotNull { tx ->
            val target = tx.groupId?.notificationSlotTargetOrNull() ?: return@mapNotNull null
            tx.id to target
        }.toMap()
    }

    private suspend fun allowancesAt(targets: List<AccountId>, at: CheckpointBlock): Map<AccountId, Boolean>? {
        return notificationSlotRepository.hasStatementAllowance(chainId, targets, at.blockHash)
            .onFailure { Timber.w(it, "Notification allowance oracle: read failed at block ${at.blockNumber}") }
            .getOrNull()
    }
}
