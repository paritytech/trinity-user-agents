package io.paritytech.polkadotapp.feature_coinage_impl.core

import io.paritytech.polkadotapp.chains.util.planksFromAmount
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageExportId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferCoinEntry
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferMemo
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferPlan
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageHandoffCommit
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.PrepareCoinageTransferUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.PreparedTransferMemo
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TotalBalanceUseCase
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import uniffi.truapi.Retry
import java.math.BigDecimal
import javax.inject.Inject
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

class InsufficientCoinageBalanceException : RuntimeException("The wallet cannot cover the amount")

@OptIn(ExperimentalTime::class)
class CorePrepareCoinageTransferUseCase @Inject constructor(
    private val core: CoinageCore,
    private val totalBalanceUseCase: TotalBalanceUseCase,
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
) : PrepareCoinageTransferUseCase {
    override suspend fun preparePlan(amount: BigDecimal): Result<TransferPlan> =
        totalBalanceUseCase.getBalance().flatMap { balance ->
            runCancellableCatching {
                val planks = chainAssetProvider.asset().planksFromAmount(amount)
                if (planks > balance.available) throw InsufficientCoinageBalanceException()
                TransferPlan(amount = amount, spendsGainingPrivacy = planks > balance.availablePrivate)
            }
        }

    override suspend fun prepareMemo(plan: TransferPlan, exportId: CoinageExportId, retryUntil: Instant): Result<PreparedTransferMemo> =
        core.units().flatMap { units ->
            core.use { coinage ->
                val cents = units.centsOf(chainAssetProvider.asset().planksFromAmount(plan.amount))
                val retry = Retry.Until(retryUntil.epochSeconds.toULong())
                val export = coinage.export(exportId.value.value, retry, cents, plan.spendsGainingPrivacy)
                val coins = export.coins.map { coin ->
                    TransferCoinEntry(privateKey = coin.secret.toDataByteArray(), valueExponent = ValueExponent(coin.exponent.toInt()))
                }
                val total = export.coins.fold(units.balanceOf(0UL)) { sum, coin -> sum + units.balanceOfExponent(coin.exponent) }
                PreparedTransferMemo(
                    exportId = exportId,
                    memo = TransferMemo(coins = coins, totalValue = total),
                    handoffCommit = CoreHandoffCommit(core, exportId),
                )
            }
        }
}

/** The export's handout: committed in the core before its keys are made durable, or released when abandoned. */
private class CoreHandoffCommit(
    private val core: CoinageCore,
    private val exportId: CoinageExportId,
) : CoinageHandoffCommit {
    override suspend fun commit(): Result<Unit> = core.use { it.commitExport(exportId.value.value) }

    override suspend fun release(): Result<Unit> = core.use { it.releaseExport(exportId.value.value) }
}
