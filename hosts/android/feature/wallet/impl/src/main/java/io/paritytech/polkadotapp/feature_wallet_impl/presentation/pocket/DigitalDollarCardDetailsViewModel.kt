package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.withAmount
import io.paritytech.polkadotapp.common.BuildConfig
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.disable
import io.paritytech.polkadotapp.common.utils.enable
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.withLoading
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.BackupProgress
import io.paritytech.polkadotapp.feature_tokens_api.presentation.mapper.TokenAmountMapper
import io.paritytech.polkadotapp.feature_wallet_impl.PocketRouter
import io.paritytech.polkadotapp.feature_wallet_impl.domain.interactor.DigitalDollarCardDetailsInteractor
import io.paritytech.polkadotapp.feature_wallet_impl.domain.model.CoinageHoldingsInfo
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.CoinageBreakdownFactory
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.CoinageScene
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.clearing
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.debug.CoinageTestDataMode
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.BalanceRestoreUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.CoinageBalanceBreakdownUiModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.CoinageUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.DigitalDollarCardDetailsUiState
import kotlinx.collections.immutable.toImmutableList
import kotlinx.collections.immutable.toImmutableMap
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

    /** The coin picked out of the grid by a tap, if any. */
    private val focusedCoin = MutableStateFlow<String?>(null)

    private val coinsState = combine(detailsVisible, focusedCoin) { expanded, focused ->
        if (expanded) focused else null
    }

    private val testData = MutableStateFlow(CoinageTestDataMode.LIVE)
    private val coinSet = MutableStateFlow(CoinageScene.CoinSet.SHIPPED)

    private val debugState = combine(testData, coinSet) { mode, set ->
        if (FeatureOption.COINAGE_DEBUG_TOOLS.isEnabled) {
            DebugState(mode, set)
        } else {
            DebugState(CoinageTestDataMode.LIVE, CoinageScene.CoinSet.SHIPPED)
        }
    }

    private val presentation = combine(detailsVisible, coinsState, debugState) { expanded, focused, debug ->
        Presentation(expanded, focused, debug)
    }

    private val holdingsFlow: Flow<Result<CoinageHoldingsInfo>> = interactor.observeHoldings()

    val coinageState: StateFlow<LoadingState<CoinageUiState>> = combine(
        holdingsFlow,
        fundInProgress,
        interactor.observeActionsEnabled(),
        presentation
    ) { holdingsResult, inProgress, actionsEnabled, shown ->
        val asset = interactor.asset()

        holdingsResult.map { holdings ->
            CoinageUiState(
                tokensState = shown.debug.testData.applied(holdings.toTokensState(asset)),
                autoFundAvailable = interactor.autoFundAvailable(),
                fundInProgress = inProgress,
                actionsEnabled = actionsEnabled,
                shareLogsEnabled = BuildConfig.TESTNET_FUND_ENABLED,
                detailsVisible = shown.expanded,
                focusedCoin = shown.focused,
                coinSet = shown.debug.coinSet,
                testData = shown.debug.testData
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

    fun onGetCashClick() = launchUnit {
        if (fundingSheetInProgress.value) return@launchUnit
        fundingSheetInProgress.enable()
        interactor.getFundingConfig()
            .onSuccess { router.openSpaSheet(it.onrampUrl) }
            .onFailure { showPresentationError(GetCashUnavailablePresentationError(it)) }
        fundingSheetInProgress.disable()
    }

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

    /**
     * Collapsed, any tap on the coins unfurls them: they are too small and too close to pick one out of.
     * Spread out, a coin picks it, and the picked coin or empty ground puts it back. Nothing among the coins
     * closes the grid — a finger aimed at a coin must not collapse it.
     */
    fun onCoinTapped(id: String?) {
        if (!detailsVisible.value) {
            detailsVisible.value = true
            return
        }

        focusedCoin.value = id.takeIf { it != focusedCoin.value }
    }

    fun onCoinsFlickedDown() {
        detailsVisible.value = true
    }

    /** The grid closes from the card's own heading, where a finger cannot miss a coin and close it instead. */
    fun onHeadingClicked() {
        if (!detailsVisible.value) return

        focusedCoin.value = null
        detailsVisible.value = false
    }

    fun onTestDataSelected(mode: CoinageTestDataMode) {
        testData.value = mode
    }

    fun onCoinSetSelected(set: CoinageScene.CoinSet) {
        coinSet.value = set
    }

    fun onShareLogsClick() = launchUnit {
        interactor.shareCoinageLogs()
            .onFailure { showPresentationError(ShareCoinageLogsFailedPresentationError(it)) }
    }

    private fun CoinageHoldingsInfo.toTokensState(asset: Chain.Asset) = CoinageUiState.TokensState(
        totalBalance = tokenAmountMapper.mapFrom(asset.withAmount(balance.total)),
        readyBalance = tokenAmountMapper.mapFrom(asset.withAmount(balance.availablePrivate)),
        clearingBalance = tokenAmountMapper.mapFrom(asset.withAmount(balance.clearing)),
        coins = CoinageBreakdownFactory.coins(holdings).toImmutableList(),
        traces = CoinageBreakdownFactory.traces(holdings) { value ->
            tokenAmountMapper.mapFrom(asset.withAmount(value))
        }.toImmutableMap(),
        breakdown = CoinageBalanceBreakdownUiModel(
            availablePrivate = tokenAmountMapper.mapFrom(asset.withAmount(balance.availablePrivate)),
            gainingPrivacy = tokenAmountMapper.mapFrom(asset.withAmount(balance.gainingPrivacy.amount)),
            pending = tokenAmountMapper.mapFrom(asset.withAmount(balance.pending)),
            canSpendGainingPrivacy = balance.gainingPrivacy.canSpendWithConfirmation
        )
    )

    private class DebugState(val testData: CoinageTestDataMode, val coinSet: CoinageScene.CoinSet)

    private class Presentation(val expanded: Boolean, val focused: String?, val debug: DebugState)

    private fun BackupProgress.toBalanceRestoreUiState(): BalanceRestoreUiState {
        return when (this) {
            is BackupProgress.Deep.Completed,
            is BackupProgress.Deep.Syncing,
            is BackupProgress.Initial.Completed -> BalanceRestoreUiState.Restore(inProgress = isInProgress())
            else -> BalanceRestoreUiState.SendCash
        }
    }
}
