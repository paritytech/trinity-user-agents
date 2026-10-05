package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.withAmount
import io.paritytech.polkadotapp.common.BuildConfig
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationThrowable
import io.paritytech.polkadotapp.common.utils.disable
import io.paritytech.polkadotapp.common.utils.enable
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.withLoading
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.BackupProgress
import io.paritytech.polkadotapp.feature_products_api.domain.FundingConfig
import io.paritytech.polkadotapp.feature_tokens_api.presentation.mapper.TokenAmountMapper
import io.paritytech.polkadotapp.feature_wallet_impl.PocketRouter
import io.paritytech.polkadotapp.feature_wallet_impl.domain.interactor.DigitalDollarCardDetailsInteractor
import io.paritytech.polkadotapp.feature_wallet_impl.domain.model.CoinageHoldingsInfo
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.CoinageBreakdownFactory
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.clearing
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.BalanceRestoreUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.CoinageBalanceBreakdownUiModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.CoinageUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.DigitalDollarCardDetailsUiState
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import javax.inject.Inject

@HiltViewModel
class DigitalDollarCardDetailsViewModel @Inject constructor(
    private val interactor: DigitalDollarCardDetailsInteractor,
    private val router: PocketRouter,
    private val tokenAmountMapper: TokenAmountMapper
) : BaseViewModel() {
    private val fundInProgress = MutableStateFlow(false)
    private val fundingSheetInProgress = MutableStateFlow(false)

    /**
     * Owned here rather than remembered in the card, so the spread grid outlives the holdings updating
     * underneath it.
     */
    private val detailsVisible = MutableStateFlow(false)

    private val holdingsFlow: Flow<Result<CoinageHoldingsInfo>> = interactor.observeHoldings()

    val coinageState: StateFlow<LoadingState<CoinageUiState>> = combine(
        holdingsFlow,
        fundInProgress,
        interactor.observeActionsEnabled(),
        detailsVisible
    ) { holdingsResult, inProgress, actionsEnabled, expanded ->
        val asset = interactor.asset()

        holdingsResult.map { holdings ->
            CoinageUiState(
                tokensState = holdings.toTokensState(asset),
                autoFundAvailable = interactor.autoFundAvailable(),
                fundInProgress = inProgress,
                actionsEnabled = actionsEnabled,
                shareLogsEnabled = BuildConfig.TESTNET_FUND_ENABLED,
                detailsVisible = expanded
            )
        }
    }
        .withLoading("CoinageCard")
        .stateIn(
            scope = this,
            started = SharingStarted.Eagerly,
            initialValue = LoadingState.Loading
        )

    val state: StateFlow<DigitalDollarCardDetailsUiState> = interactor.observeBackupProgress()
        .map { DigitalDollarCardDetailsUiState(balanceRestore = it.toBalanceRestoreUiState()) }
        .stateIn(
            scope = this,
            started = SharingStarted.Eagerly,
            initialValue = DigitalDollarCardDetailsUiState(
                balanceRestore = BalanceRestoreUiState.NotDetermined
            )
        )

    fun onGetCashClick() = openFundingSheet(FundingConfig::onrampUrl, ::GetCashUnavailablePresentationError)

    fun onWithdrawClick() = openFundingSheet(FundingConfig::offrampUrl, ::WithdrawUnavailablePresentationError)

    fun onSendClick() {
        router.openSendPayment()
    }

    fun onAutoFundClick() = launchUnit {
        if (fundInProgress.value) return@launchUnit
        fundInProgress.enable()
        interactor.testnetFund()
            .onFailure { showPresentationError(AutoFundFailedPresentationError(it)) }
        fundInProgress.disable()
    }

    fun onBackupUpdateClick() {
        interactor.startDeepSearch()
    }

    fun onBackupCloseClick() {
        interactor.markBackupCompleted()
    }

    fun onDetailsToggled() {
        detailsVisible.value = !detailsVisible.value
    }

    fun onShareLogsClick() = launchUnit {
        interactor.shareCoinageLogs()
            .onFailure { showPresentationError(ShareCoinageLogsFailedPresentationError(it)) }
    }

    private fun openFundingSheet(
        urlOf: (FundingConfig) -> String,
        error: (Throwable) -> PresentationThrowable
    ) = launchUnit {
        if (fundingSheetInProgress.value) return@launchUnit
        fundingSheetInProgress.enable()
        interactor.getFundingConfig()
            .onSuccess { router.openSpaSheet(urlOf(it)) }
            .onFailure { showPresentationError(error(it)) }
        fundingSheetInProgress.disable()
    }

    private fun CoinageHoldingsInfo.toTokensState(asset: Chain.Asset) = CoinageUiState.TokensState(
        totalBalance = tokenAmountMapper.mapFrom(asset.withAmount(balance.total)),
        readyBalance = tokenAmountMapper.mapFrom(asset.withAmount(balance.availablePrivate)),
        clearingBalance = tokenAmountMapper.mapFrom(asset.withAmount(balance.clearing)),
        coins = CoinageBreakdownFactory.coins(holdings).toImmutableList(),
        breakdown = CoinageBalanceBreakdownUiModel(
            availablePrivate = tokenAmountMapper.mapFrom(asset.withAmount(balance.availablePrivate)),
            gainingPrivacy = tokenAmountMapper.mapFrom(asset.withAmount(balance.gainingPrivacy.amount)),
            pending = tokenAmountMapper.mapFrom(asset.withAmount(balance.pending)),
            canSpendGainingPrivacy = balance.gainingPrivacy.canSpendWithConfirmation
        )
    )

    private fun BackupProgress.toBalanceRestoreUiState(): BalanceRestoreUiState {
        return when (this) {
            is BackupProgress.Deep.Completed,
            is BackupProgress.Deep.Syncing,
            is BackupProgress.Initial.Completed -> BalanceRestoreUiState.Restore(inProgress = isInProgress())
            else -> BalanceRestoreUiState.SendCash
        }
    }
}
