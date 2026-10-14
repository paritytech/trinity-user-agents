package io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase

import io.novasama.substrate_sdk_android.encrypt.keypair.Keypair
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.common.utils.flattenResult
import io.paritytech.polkadotapp.common.utils.mapAsync
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinPrivateKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.deriveKeypair
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionRequest
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainCoinInfo
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogD
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogW
import io.paritytech.polkadotapp.feature_coinage_impl.domain.model.CoinageTransaction
import io.paritytech.polkadotapp.feature_coinage_impl.domain.model.mintCoin
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies.builders.ClaimExtrinsicBuilder
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission.ClaimSubmissionParams
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission.CoinageSubmissionParams
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import javax.inject.Inject
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

interface CoinageTransferSubmissionUseCase {
    /**
     * Register a transfer of requested coins into freshly created accounts
     * This does not perform any retries itself and returns as soon as **registration** is completed: a claim
     * proven unable to land is built again by the engine, into the same account, until [retryUntil] has
     * passed with its coin gone from the chain.
     * Status monitoring should be done via [CoinageTransactionService.subscribeOperationGroupStatuses] for the given [groupId]
     */
    @OptIn(ExperimentalTime::class)
    suspend operator fun invoke(
        coinKeys: List<CoinPrivateKey>,
        coinsInfo: Map<AccountId, OnChainCoinInfo>,
        groupId: CoinageOperationGroupId,
        retryUntil: Instant,
    ): Result<Unit>
}

/**
 * Claims coins a peer handed us: each one is transferred to a fresh address of ours.
 *
 * The peer's key is a `Received` input — never a local asset — so the ledger can hold it against exactly one
 * claim without us ever having minted it.
 */
@OptIn(ExperimentalTime::class)
class RealCoinageTransferSubmissionUseCase @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    private val claimExtrinsicBuilder: ClaimExtrinsicBuilder,
    private val transactionService: CoinageTransactionService,
    private val coinageTransactionFactory: CoinageTransaction.Factory,
) : CoinageTransferSubmissionUseCase {
    override suspend operator fun invoke(
        coinKeys: List<CoinPrivateKey>,
        coinsInfo: Map<AccountId, OnChainCoinInfo>,
        groupId: CoinageOperationGroupId,
        retryUntil: Instant,
    ): Result<Unit> {
        val keyed = coinKeys.map { key ->
            val keypair = key.deriveKeypair()
            KeyedCoin(key, keypair, keypair.publicKey.toDataByteArray())
        }

        // The crowd the arriving coins hide in: every coin this operation actually moves, counted before any
        // claim is built so all of them record the same bundle.
        val bundleSize = keyed.count { coinsInfo.containsKey(it.accountId) }

        val claims = keyed.mapAsync { coin ->
            coinsInfo[coin.accountId]?.let { info ->
                buildClaim(ValueExponent(info.value), coin, groupId, bundleSize, retryUntil)
            } ?: run {
                coinageLogW("Claim skipped, no coin on chain group=${groupId.value} coin=${coin.accountId}")
                Result.success(null)
            }
        }
            .flattenResult()
            .getOrElse { return Result.failure(it) }
            .filterNotNull()

        if (claims.isEmpty()) return Result.success(Unit)

        coinageLogD("Claims registering group=${groupId.value} claims=${claims.size}")

        return transactionService.submitTransactions(claims, groupId).coerceToUnit()
    }

    /**
     * Each claim is signed by the peer's own key, so these cannot be one nonce-sequenced batch the way a
     * group of our own transactions can. They are built independently and registered together, which is what
     * the ledger cares about: either the whole claim group is recorded or none of it is.
     */
    private suspend fun buildClaim(
        valueExponent: ValueExponent,
        coin: KeyedCoin,
        groupId: CoinageOperationGroupId,
        bundleSize: Int,
        retryUntil: Instant,
    ): Result<CoinageTransactionRequest> {
        val chain = chainAssetProvider.chain()
        val transaction = coinageTransactionFactory.newTransaction()
        val source = coin.accountId

        // Nothing is known about where this coin has been: its age arrives later, from the ownership
        // subscription, and only then can its history be written. See CoinPresenceSyncService.
        val provenance = CoinProvenance.incoming(bundleSize)

        val destination = transaction.mintCoin(valueExponent, provenance)
            .getOrElse { return Result.failure(it) }
        transaction.consumeReceivedCoin(source)
        val assets = transaction.build()

        coinageLogD("Claim built group=${groupId.value} coin=$source value=${valueExponent.value}")

        return claimExtrinsicBuilder.build(chain, coin.keypair, destination.accountId).map { extrinsic ->
            CoinageTransactionRequest(
                extrinsic = extrinsic,
                inputs = assets.inputs,
                outputs = assets.outputs,
                policy = CoinageSubmissionParams.claimPolicy(ClaimSubmissionParams(retryUntil, coin.key)),
            )
        }
    }

    private class KeyedCoin(
        val key: CoinPrivateKey,
        val keypair: Keypair,
        val accountId: AccountId,
    )
}
