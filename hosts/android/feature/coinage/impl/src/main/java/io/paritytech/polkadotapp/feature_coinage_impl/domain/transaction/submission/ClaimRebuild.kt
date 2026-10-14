package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission

import io.novasama.substrate_sdk_android.encrypt.keypair.Keypair
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.flattenResult
import io.paritytech.polkadotapp.common.utils.mapAsync
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.deriveKeypair
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.EntryAssets
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies.builders.ClaimExtrinsicBuilder
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.data.EnrichedSendableExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.ScheduledDurableTx
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

/**
 * A claim of a coin a peer handed us, into the coin its first attempt recorded. Minting into the same coin is what
 * lets a payment we already made out of it keep waiting on it. Signed with the peer's key from the params, which
 * only the payment message carries.
 */
class ClaimRebuild @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    private val coinRepository: CoinRepository,
    private val claimExtrinsicBuilder: ClaimExtrinsicBuilder,
) : CoinageRebuild<ClaimRebuild.Claim, AccountId> {
    class Claim(val keypair: Keypair, val source: AccountId, val destination: AccountId)

    /** A claim's failures are always worth weighing: the peer's coin is money nothing else will collect. */
    override fun termsOf(params: DataByteArray): RebuildTerms? =
        CoinageSubmissionParams.decodeClaim(params).getOrNull()?.let { RebuildTerms(it.retryUntil, retriesFailures = true) }

    override suspend fun resolve(
        transactions: List<ScheduledDurableTx>,
        assets: Map<DurableTxId, EntryAssets>,
    ): Map<DurableTxId, Claim> = transactions.mapNotNull { tx ->
        val destination = assets[tx.id]?.outputs?.singleOrNull()?.publicKey ?: return@mapNotNull null
        val params = CoinageSubmissionParams.decodeClaim(tx.policy.params).getOrNull() ?: return@mapNotNull null
        val keypair = params.receivedKey.deriveKeypair()

        tx.id to Claim(keypair, keypair.publicKey.toDataByteArray(), destination)
    }.toMap()

    override fun inputsOf(transaction: Claim): Set<AccountId> = setOf(transaction.source)

    override suspend fun presence(inputs: Set<AccountId>): Flow<Set<AccountId>> =
        coinRepository.subscribeCoinPresence(chainAssetProvider.chainId(), inputs.toList())

    override suspend fun build(transactions: List<Claim>): Result<List<EnrichedSendableExtrinsic>> {
        val chain = chainAssetProvider.chain()

        return transactions.mapAsync { claimExtrinsicBuilder.build(chain, it.keypair, it.destination) }.flattenResult()
    }
}
