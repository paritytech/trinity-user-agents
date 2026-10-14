package io.paritytech.polkadotapp.feature_fund_impl.domain

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.allAssets
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.withAsset
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.amountFromPlanks
import io.paritytech.polkadotapp.chains.util.fullId
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.common.utils.filterResultSuccessNotNull
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.getDepositAccount
import io.paritytech.polkadotapp.feature_balances_api.domain.model.TokenBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.OnboardingUseCase
import io.paritytech.polkadotapp.feature_fund_api.domain.AutoConvertDepositService
import io.paritytech.polkadotapp.feature_fund_api.domain.model.AutoConvertDeposit
import io.paritytech.polkadotapp.feature_fund_api.domain.model.DepositTerms
import io.paritytech.polkadotapp.feature_fund_api.domain.model.DepositTerms.ConversionRate
import io.paritytech.polkadotapp.feature_fund_impl.data.BalanceChangeTracker
import io.paritytech.polkadotapp.feature_fund_impl.data.FundsConverter
import io.paritytech.polkadotapp.feature_fund_impl.data.model.ConversionProgress
import io.paritytech.polkadotapp.feature_fund_impl.data.model.PossibleFundConversion
import io.paritytech.polkadotapp.feature_fund_impl.data.model.depositedAmount
import io.paritytech.polkadotapp.feature_fund_impl.data.model.expectedConvertedAmount
import io.paritytech.polkadotapp.feature_prices_api.domain.GetPriceUseCase
import io.paritytech.polkadotapp.feature_prices_api.domain.model.FiatAmount
import io.paritytech.polkadotapp.feature_prices_api.domain.model.priceOf
import io.paritytech.polkadotapp.feature_prices_api.domain.model.sumOfOrThrow
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapExecutionEstimate
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapFee
import io.paritytech.polkadotapp.feature_swap_api.domain.model.swapRate
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.asSignerSource
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.decimalAmount
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.transform
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton

@Singleton // Important - stateful service!
internal class RealAutoConvertDepositService @Inject constructor(
    private val balanceChangeTracker: BalanceChangeTracker,
    private val fundsConverter: FundsConverter,
    private val accountRepository: AccountRepository,
    private val chainRegistry: ChainRegistry,
    @param:DigitalDollarChainAssetProvider private val assetProvider: ChainAssetProvider,
    private val coroutineDispatchers: CoroutineDispatchers,
    private val getPriceUseCase: GetPriceUseCase,
    private val onboardingUseCase: OnboardingUseCase
) : AutoConvertDepositService {
    override val currentDeposit = MutableStateFlow<AutoConvertDeposit?>(null)

    context(scope: ComputationalScope)
    override suspend fun startObserveAndConvert() {
        fundsConverter.launchSync()
            .launchIn(scope)

        startSwapObserveAndConvert()
    }

    context(scope: ComputationalScope)
    private fun startSwapObserveAndConvert() = scope.launch {
        // Keep track of failed conversion checks to avoid checking same balance over and over in case
        // the balance has never left origin chain
        val failedBalances = mutableSetOf<String>()

        while (true) {
            val destinationAsset = assetProvider.asset()
            val destinationChain = assetProvider.chain()

            val depositAccount = accountRepository.getDepositAccount()
            val destinationAccount = accountRepository.getDepositAccount().accountIdIn(destinationChain)
            val assets = chainRegistry.allAssets()

            Timber.d("[Airdrop] deposit: tracking deposit-account balance changes destChain=${destinationChain.id} destAsset=${destinationAsset.id}")

            // We collect balances from subscription until we find valid candidate
            // Then we stop to do the transfer. We do not keep subscription to avoid potentially
            // irrelevant updaters that happened as the result of swap intermediate segment execution
            val (checkedConversion, balance) = balanceChangeTracker.balanceChanges(depositAccount, assets)
                .filter { it.transferable.isPositive() } // Filter out most of the not relevant balances
                .onEach { Timber.d("[Airdrop] deposit: positive funding balance asset=${it.token.id} transferable=${it.transferable}") }
                .mapNotNull {
                    if (it.stateSnapshot() in failedBalances) {
                        Timber.d("[Airdrop] deposit: balance already attempted with failure: $it")
                        null
                    } else {
                        it
                    }
                }
                .map { balance ->
                    if (balance.token.id != destinationAsset.id) {
                        fundsConverter.checkConversionPossible(balance, destinationAsset, depositAccount, destinationAccount)
                            .logFailure("Failed to check conversion candidate: $balance")
                            .onFailure { failedBalances.add(balance.stateSnapshot()) }
                            .map { it to balance }
                    } else {
                        Result.success(null to balance)
                    }
                }
                .filterResultSuccessNotNull()
                .first()

            if (checkedConversion != null) {
                Timber.d("[Airdrop] deposit: balance needs conversion → executeCheckedConversion")
                executeCheckedConversion(checkedConversion)
            } else {
                val actualBalance = balance.transferable
                val initialDeposit = actualBalance.createInitialDeposit()
                Timber.d("[Airdrop] deposit: balance already destination asset → performOnboarding amount=$actualBalance")
                performOnboarding(actualBalance, initialDeposit) { currentDeposit.value = it }
            }
                .onFailure { Timber.w(it, "[Airdrop] deposit: convert/onboard FAILED for $balance"); failedBalances.add(balance.stateSnapshot()) }
        }
    }

    context(scope: ComputationalScope)
    override fun initiateDepositTermsWarmUp() {
        fundsConverter.initiateConversionTermsWarmUp()
    }

    private fun TokenBalance.stateSnapshot(): String {
        return buildString {
            append(token.chainId).append(":")
            append(token.id).append("")
            append(transferable)
        }
    }

    context(scope: ComputationalScope)
    override suspend fun depositTerms(chainAsset: Chain.Asset): Result<DepositTerms> {
        val destinationAsset = assetProvider.asset()
        val destinationChain = assetProvider.chain()

        val depositAccount = accountRepository.getDepositAccount()
        val walletAccountId = accountRepository.getDepositAccount().accountIdIn(destinationChain)

        return fundsConverter.getConversionTerms(chainAsset, destinationAsset, depositAccount, walletAccountId)
            .mapCatching {
                DepositTerms(
                    minDeposit = it.minDepositAmount,
                    conversionRate = ConversionRate(
                        from = chainAsset,
                        to = destinationAsset,
                        rate = it.midSizeDepositQuote.swapRate()
                    ),
                    estimatedFee = convertFeeToFiat(it.fee)
                )
            }
    }

    private suspend fun convertFeeToFiat(fee: SwapFee): FiatAmount {
        return withContext(coroutineDispatchers.io) {
            val basicFees = fee.feeComponents()
            val chainAssets = basicFees.map { it.asset }
            val priceLookup = getPriceUseCase.getPrices(chainAssets)

            basicFees.sumOfOrThrow { basicFee ->
                val price = priceLookup[basicFee.asset.fullId]
                price.priceOf(basicFee.decimalAmount())
            }
        }
    }

    context(scope: ComputationalScope)
    private suspend fun executeCheckedConversion(checkedConversion: PossibleFundConversion): Result<Unit> {
        val executionEstimate = checkedConversion.quote.executionEstimate
        val initialDepositInstance = checkedConversion.createInitialDeposit(executionEstimate)

        Timber.d("Got valid fund candidate: $checkedConversion")

        val conversionFlow = fundsConverter.performConversion(checkedConversion)
            .transform {
                when (it) {
                    is ConversionProgress.Done -> {
                        val actualDeposited = it.actualDeposited
                        emit(initialDepositInstance.updateProgress())

                        Timber.d("Got Done update: ${currentDeposit.value}")

                        performOnboarding(actualDeposited, initialDepositInstance) { emit(it) }

                        Timber.d("Got Swap Completed update: ${currentDeposit.value}")

                        emit(null)
                    }

                    is ConversionProgress.Failed -> {
                        emit(initialDepositInstance.failed(it.error))

                        Timber.d("Got Failed update: ${currentDeposit.value}")

                        emit(null)
                    }

                    is ConversionProgress.SwapInProgress -> {
                        val newCompletesAt = executionEstimate.completesAtWhenExecuting(it.swapProgressStep.index)
                        emit(initialDepositInstance.updateProgress(newCompletesAt))

                        Timber.d("Got SwapInProgress update: ${currentDeposit.value}")
                    }

                    // TODO estimate transfer time as well?
                    ConversionProgress.TransferToRecipient -> {}
                }
            }

        var failure: AutoConvertDeposit.Status.Failure? = null

        conversionFlow
            .onEach {
                currentDeposit.value = it

                val status = it?.status

                if (status is AutoConvertDeposit.Status.Failure) {
                    failure = status
                }
            }
            .collect()

        return if (failure != null) {
            Result.failure(failure.reason)
        } else {
            Result.success(Unit)
        }
    }

    private suspend fun performOnboarding(
        balance: Balance,
        initialDepositInstance: AutoConvertDeposit,
        emitState: suspend (AutoConvertDeposit?) -> Unit
    ): Result<Unit> {
        val account = accountRepository.getDepositAccount()
        val amount = assetProvider.asset().amountFromPlanks(balance)

        Timber.d("[Airdrop] deposit: onboarding amount=$amount into Coinage")
        return onboardingUseCase.onboard(amount, account.asSignerSource())
            .onSuccess {
                Timber.d("[Airdrop] deposit: onboarding COMPLETED — Coinage balance should now reflect it")
                emitState(initialDepositInstance.done())
            }
            .onFailure {
                Timber.w(it, "[Airdrop] deposit: onboarding FAILED")
                emitState(initialDepositInstance.failed(it))
            }
            .coerceToUnit()
    }

    private fun PossibleFundConversion.createInitialDeposit(estimate: SwapExecutionEstimate): AutoConvertDeposit {
        return AutoConvertDeposit.new(
            depositedAmount(),
            expectedConvertedAmount(),
            estimate.completesAtFromScratch()
        )
    }

    private suspend fun Balance.createInitialDeposit(): AutoConvertDeposit {
        val asset = assetProvider.asset()
        return AutoConvertDeposit.new(
            withAsset(asset),
            withAsset(asset),
            System.currentTimeMillis()
        )
    }
}
