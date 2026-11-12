package io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase

import io.paritytech.polkadotapp.bandersnatch_crypto.memberKey
import io.paritytech.polkadotapp.bandersnatch_crypto.sign
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.chains.multiNetwork.connection.withConnectionEnabled
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.VoucherAllocator
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclingStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.isInRecycler
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageInput
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionRequest
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionState
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageRecyclingUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.coinage
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.loadRecyclerWithCoin
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.VoucherRingDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.signer.origins.CoinageTransactionOrigins
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogD
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogI
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogW
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.data.MultiExtrinsicBuilder
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import javax.inject.Inject

class RealCoinageRecyclingUseCase @Inject constructor(
    private val coinRepository: CoinRepository,
    private val voucherAllocator: VoucherAllocator,
    private val voucherRingDerivation: VoucherRingDerivation,
    private val coinageTransactionOrigins: CoinageTransactionOrigins,
    private val chainConnectionRefCounter: ChainConnectionRefCounter,
    private val chainRegistry: ChainRegistry,
    private val extrinsicService: ExtrinsicService,
    private val transactionService: CoinageTransactionService,
    private val voucherRepository: VoucherRepository,
    private val dispatchers: CoroutineDispatchers,
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider
) : CoinageRecyclingUseCase {
    /**
     * One group of load-recycler transactions, one per coin.
     *
     * Every extrinsic is built before any is registered so their nonces stay sequenced; registration then
     * locks each coin, which is what the local spent-marking used to stand in for. A coin whose transaction
     * never lands is released when the recovery pass fails it, so there is no rollback to run here.
     */
    override suspend fun recycle(coins: List<Coin>): Result<Unit> =
        recycle(coins, CoinageOperationGroupId.generateNew())

    override suspend fun recycle(coins: List<Coin>, groupId: CoinageOperationGroupId): Result<Unit> {
        val alreadySubmitted = transactionService.getOperationGroupStatuses(groupId)
            .getOrElse { return Result.failure(it) }

        if (alreadySubmitted.isNotEmpty()) {
            coinageLogD("Recycling already submitted group=${groupId.value} transactions=${alreadySubmitted.size}")
            return Result.success(Unit)
        }

        val idle = coins.filterIdle()
        if (idle.isEmpty()) {
            coinageLogD("Recycling has nothing idle to submit coins=${coins.size}")
            return Result.success(Unit)
        }

        val coinsWithVouchers = idle.allocateVouchersForCoins()
        if (coinsWithVouchers.isEmpty()) {
            coinageLogW("Recycling has nothing to submit coins=${coins.size} vouchers=0")
            return Result.success(Unit)
        }

        coinageLogD("Recycling allocated vouchers=${coinsWithVouchers.size} coins=${coins.size}")

        val chainId = chainAssetProvider.chainId()

        return chainConnectionRefCounter.withConnectionEnabled(chainId, "CoinageRecycling") {
            val chain = chainRegistry.getChain(chainId)

            submitRecycleGroup(chain, coinsWithVouchers, groupId)
        }
    }

    override fun observeRecyclingStatus(groupId: CoinageOperationGroupId): Flow<RecyclingStatus> = combine(
        transactionService.subscribeOperationGroupStatuses(groupId),
        voucherRepository.subscribeAllVouchers(),
    ) { states, vouchers ->
        states.toRecyclingStatus(vouchers)
    }.distinctUntilChanged()

    private fun List<CoinageTransactionState>.toRecyclingStatus(vouchers: List<RecyclerVoucher>): RecyclingStatus {
        if (isEmpty() || any { it.status == DurableTxStatus.FAILURE }) return RecyclingStatus.Incomplete
        if (!all { it.status.isArrived }) return RecyclingStatus.Pending

        val vouchersByIndex = vouchers.associateBy { it.ringVrfKeyIndex }
        val recycled = flatMap { it.outputs }
            .filterIsInstance<OwnAsset.Voucher>()
            .map { vouchersByIndex[it.ringVrfIndex] }

        // Arrival is read at the best head, the recycler location lags behind it, and an unload needs the latter.
        if (recycled.any { it == null || !it.isInRecycler() }) return RecyclingStatus.Pending

        return RecyclingStatus.AllRecycled(
            vouchers = recycled.filterNotNull(),
            finalized = all { it.status == DurableTxStatus.FINALIZED_SUCCESS },
        )
    }

    private suspend fun submitRecycleGroup(
        chain: Chain,
        coinsWithVouchers: List<Pair<Coin, RecyclerVoucher>>,
        groupId: CoinageOperationGroupId,
    ): Result<Unit> = runCatching {
        val extrinsics = extrinsicService.buildExtrinsics(chain) {
            coinsWithVouchers.forEach { (coin, voucher) -> buildLoadRecyclerExtrinsic(coin, voucher) }
        }.getOrThrow()

        coinageLogI("Recycling submitting group=${groupId.value} coins=${coinsWithVouchers.size}")

        val requests = extrinsics.mapIndexed { index, extrinsic ->
            val (coin, voucher) = coinsWithVouchers[index]

            CoinageTransactionRequest(
                extrinsic = extrinsic,
                inputs = listOf(CoinageInput.Coin.Own(coin.derivationIndex)),
                outputs = listOf(OwnAsset.Voucher(voucher.ringVrfKeyIndex)),
                policy = null,
            )
        }

        transactionService.submitTransactions(requests, groupId).getOrThrow()
    }

    /**
     * Idempotency: only allow to recycle coins that are not already recycling.
     * Or, in general: not known as inputs to [transactionService]
     */
    private suspend fun List<Coin>.filterIdle(): List<Coin> {
        val assets = map { OwnAsset.Coin(it.derivationIndex) }
        val states = transactionService.getAssetStates(assets)
            .logFailure("Failed to read asset states for recycling")
            .getOrElse { return emptyList() }

        return filter { coin ->
            val ownAsset = OwnAsset.Coin(coin.derivationIndex)
            states.getAssetStateOf(ownAsset).isFree
        }
    }

    private suspend fun List<Coin>.allocateVouchersForCoins() = mapNotNull { coin ->
        val voucher = voucherAllocator.allocate(coin.valueExponent).getOrElse { error ->
            coinageLogW(
                "Coin not recycled, voucher allocation failed" +
                    " coin=${coin.derivationIndex} value=${coin.valueExponent.value}: ${error.message}"
            )
            return@mapNotNull null
        }

        coin to voucher
    }

    context(builder: MultiExtrinsicBuilder)
    private suspend fun buildLoadRecyclerExtrinsic(coin: Coin, voucher: RecyclerVoucher) {
        val keypair = voucherRingDerivation.deriveBandersnatch(voucher.ringVrfKeyIndex)
        val origin = coinageTransactionOrigins.createAsCoinOrigin(coin)

        builder.extrinsic(origin = origin) {
            coinage.loadRecyclerWithCoin(
                memberKey = keypair.memberKey().value,
                proofOfOwnership = keypair.sign(coin.accountId.value)
            )
        }
    }
}
