package io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount

import androidx.lifecycle.SavedStateHandle
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.withAmount
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.chains.util.amountFromPlanks
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.validation.onError
import io.paritytech.polkadotapp.common.domain.validation.onSuccess
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.presentation.validation.ValidationMixin
import io.paritytech.polkadotapp.common.utils.gate
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReport
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.common.utils.progressStallReport.launchWithDiagnostics
import io.paritytech.polkadotapp.common.utils.shareInBackground
import io.paritytech.polkadotapp.design.configs.colors.AvatarColorScheme
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddress
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddressParcel
import io.paritytech.polkadotapp.feature_balances_api.presentation.provider.BalanceFlowAvailableBalanceProvider
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentPlan
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.StrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferPlan
import io.paritytech.polkadotapp.feature_tokens_api.presentation.amountinput.AmountInput
import io.paritytech.polkadotapp.feature_tokens_api.presentation.amountinput.AmountInputMixin
import io.paritytech.polkadotapp.feature_tokens_api.presentation.amountinput.create
import io.paritytech.polkadotapp.feature_tokens_api.presentation.mapper.TokenAmountMapper
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.RoundPrecision
import io.paritytech.polkadotapp.feature_wallet_api.presentation.enterAmount.AmountPreset
import io.paritytech.polkadotapp.feature_wallet_api.presentation.enterAmount.SendEnterAmountPayload
import io.paritytech.polkadotapp.feature_wallet_api.presentation.enterAmount.TransferMethodPayload
import io.paritytech.polkadotapp.feature_wallet_impl.BuildConfig
import io.paritytech.polkadotapp.feature_wallet_impl.PocketRouter
import io.paritytech.polkadotapp.feature_wallet_impl.domain.model.AvailableToSendAmount
import io.paritytech.polkadotapp.feature_wallet_impl.domain.model.SendPlan
import io.paritytech.polkadotapp.feature_wallet_impl.domain.model.TransferMethod
import io.paritytech.polkadotapp.feature_wallet_impl.domain.model.reachablePlanks
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.SendEnterAmountUiState.SendProgress
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.SendEnterAmountUiState.SendProgress.Settling.Stage
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.domain.SendEnterAmountInteractor
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.domain.SendState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.domain.SendValidationPayload
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.domain.asSendError
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.transactionResult.TransactionSuccessPayload
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapLatest
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import java.math.BigDecimal
import javax.inject.Inject

@HiltViewModel
class SendEnterAmountViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    amountInputMixinFactory: AmountInputMixin.Factory,
    private val walletRouter: PocketRouter,
    private val interactor: SendEnterAmountInteractor,
    private val tokenAmountMapper: TokenAmountMapper,
) : BaseViewModel(), SendEnterAmountContract {
    private val sendProgress = MutableStateFlow<SendProgress>(SendProgress.Idle)

    private val tokenBalance: Flow<AvailableToSendAmount> = interactor.tokenBalance()
        .gate(sendProgress.map { it is SendProgress.Idle })
        .shareInBackground()

    // Reachable, not spendable: capping the input at the spendable balance would disable
    // Send for exactly the amounts the confirmation exists to allow.
    private val balanceFlow: Flow<BigDecimal> = tokenBalance.map { it.reachablePlanks() }

    private val amountInputMixin = amountInputMixinFactory.create(
        roundPrecision = RoundPrecision.FIAT,
        asset = { interactor.asset() },
        availableBalanceProvider = BalanceFlowAvailableBalanceProvider(balanceFlow)
    )

    private val payload: SendEnterAmountPayload = savedStateHandle.getPayload()
    private val recipientInfo: RecipientInfo = payload.transferMethod.toRecipientInfo()
    private val transferMethod: TransferMethod = payload.transferMethod.toDomain()
    private val isAmountLocked: Boolean = payload.amountPreset?.lockAmount == true

    init {
        payload.amountPreset?.let { applyPreset(it) }
    }

    private val debugPlanFlow: Flow<SendPlanDebugInfo?> = combine(
        amountInputMixin.value,
        interactor.observeDebugWidgetsEnabled()
    ) { mixinValue, widgetsEnabled ->
        mixinValue to widgetsEnabled
    }.mapLatest { (mixinValue, widgetsEnabled) ->
        val amount = mixinValue.input.input.toBigDecimalOrNull()
        if (amount == null || amount <= BigDecimal.ZERO || !BuildConfig.DEBUG || !widgetsEnabled) {
            null
        } else {
            val plan = interactor.plan(amount, transferMethod)
            plan?.toDebugInfo()
        }
    }.shareInBackground()

    override val sendValidationMixin = ValidationMixin.create()

    override val stalenessReport = StalenessReport(this)

    private val balanceSplit = tokenBalance
        .map { balance ->
            val spendable = tokenAmountMapper.mapFrom(balance.chainAsset.withAmount(balance.spendable))
            val offerable = balance.offerable?.let { tokenAmountMapper.mapFrom(balance.chainAsset.withAmount(it)) }

            spendable to offerable
        }
        .shareInBackground()

    override val state = combine(
        amountInputMixin.value,
        amountInputMixin.availableBalance,
        sendProgress,
        debugPlanFlow,
        balanceSplit,
    ) { inputMixinValue, availableBalance, progress, debugPlan, (spendable, gainingPrivacy) ->
        val inputNum = inputMixinValue.input.input.toBigDecimalOrNull()

        val isPositiveAmount = inputNum?.let { it > BigDecimal.ZERO } ?: false
        val showBalanceError = inputNum?.let { it > availableBalance.amount } ?: false
        val isSendEnabled = isPositiveAmount && !showBalanceError

        LoadingState.Loaded(
            SendEnterAmountUiState(
                input = inputMixinValue.input.input,
                sendProgress = progress,
                available = availableBalance,
                spendable = spendable,
                gainingPrivacy = gainingPrivacy,
                recipient = recipientInfo.display,
                recipientType = recipientInfo.type,
                recipientAvatarColor = recipientInfo.avatarColor,
                showBalanceError = showBalanceError,
                isAmountPositive = isPositiveAmount,
                isSendEnabled = isSendEnabled,
                isAmountLocked = isAmountLocked,
                debugPlanInfo = debugPlan,
            )
        )
    }.stateIn(
        scope = this,
        started = SharingStarted.Eagerly,
        initialValue = LoadingState.Loading
    )

    override fun onNewInput(value: String) {
        if (isAmountLocked) return
        amountInputMixin.setNewInput(AmountInput(value, AmountInput.Origin.USER))
    }

    override fun onConfirmClick() {
        // The validation can present a confirmation sheet, but it opens no region, so the time the user
        // spends reading it cannot trip the report's reveal timer.
        launchWithDiagnostics(stalenessReport) {
            sendProgress.value = SendProgress.Submitting

            val amount = amountInputMixin.value.first().amount

            val payload = SendValidationPayload(amount.amount, transferMethod)

            val validationResult = sendValidationMixin.runValidation(interactor.sendValidation, payload)
            validationResult.onSuccess { sendValidatedTransfer(it) }
            validationResult.onError { showPresentationError(it.asSendError().toPresentationError()) }

            sendProgress.value = SendProgress.Idle
        }
    }

    context(diagnostics: StalenessReportCollector)
    private suspend fun sendValidatedTransfer(payload: SendValidationPayload) {
        interactor.send(payload.value, payload.transferMethod)
            .collect { state ->
                when (state) {
                    is SendState.Detecting -> sendProgress.value = SendProgress.Settling(Stage.DETECTING)
                    is SendState.Detected -> sendProgress.value = SendProgress.Settling(Stage.DETECTED)
                    is SendState.Complete -> handleTransactionSuccess(state)
                    is SendState.Failed -> handleTransactionFailure(state.error)
                }
            }
    }

    private fun handleTransactionSuccess(state: SendState.Complete) {
        if (payload.showTransactionResult) {
            val unfinalizedCoins = state.unfinalizedCoins?.map { it.value }
            walletRouter.openSuccess(TransactionSuccessPayload(unfinalizedCoins))
        } else {
            walletRouter.back()
        }
    }

    private fun handleTransactionFailure(error: Throwable) {
        if (payload.showTransactionResult) {
            walletRouter.openFailure()
        } else {
            showPresentationError(error.asSendError().toPresentationError())
            walletRouter.back()
        }
    }

    override fun onBackClick() {
        walletRouter.back()
    }

    private fun applyPreset(preset: AmountPreset) {
        launch {
            val decimal = interactor.asset().amountFromPlanks(preset.amount.intoBalance())
            amountInputMixin.setNewInput(AmountInput(decimal.toPlainString(), AmountInput.Origin.INTERNAL))
        }
    }
}

private data class RecipientInfo(
    val display: String?,
    val type: ExtractedAddress.DisplayType?,
    val avatarColor: AvatarColorScheme,
)

private fun TransferMethodPayload.toRecipientInfo(): RecipientInfo = when (this) {
    is TransferMethodPayload.CoinsViaChat -> recipient.toRecipientInfo()
    is TransferMethodPayload.UnloadIntoExternal -> recipient.toRecipientInfo()
    is TransferMethodPayload.CoinsViaSubmitter ->
        RecipientInfo(
            display = recipientLabel,
            type = ExtractedAddress.DisplayType.USERNAME,
            avatarColor = AvatarColorScheme.from(submitterPayload),
        )
}

private fun ExtractedAddressParcel.toRecipientInfo(): RecipientInfo =
    RecipientInfo(display, type, AvatarColorScheme.from(accountId))

private fun TransferMethodPayload.toDomain(): TransferMethod = when (this) {
    is TransferMethodPayload.CoinsViaChat -> TransferMethod.CoinsViaChat(recipient.accountId.intoAccountId())
    is TransferMethodPayload.UnloadIntoExternal -> TransferMethod.UnloadIntoExternal(recipient.accountId.intoAccountId())
    is TransferMethodPayload.CoinsViaSubmitter -> TransferMethod.CoinsViaSubmitter(submitterId, submitterPayload)
}

private fun SendPlan.toDebugInfo(): SendPlanDebugInfo = when (this) {
    is SendPlan.Coinage -> plan.toDebugInfo()
    is SendPlan.External -> plan.toDebugInfo()
}

private fun TransferPlan.toDebugInfo(): SendPlanDebugInfo.Coinage {
    val strategyName = when (strategyType) {
        is StrategyType.ExactCoins -> "ExactCoins"
        is StrategyType.Split -> "Split"
        is StrategyType.UnloadAndSplit -> "UnloadAndSplit"
    }
    val details = buildList {
        when (val st = strategyType) {
            is StrategyType.UnloadAndSplit -> {
                add("Vouchers to unload:")
                st.vouchersToUnload.forEach { v ->
                    add("  idx=${v.ringVrfKeyIndex.item}  exp=2^${v.recyclerValue.value}")
                }
            }

            is StrategyType.Split -> {
                add("Coins for split:")
                add("  idx=${st.splitFrom.derivationIndex.item}  exp=2^${st.splitFrom.valueExponent.value}")
            }

            is StrategyType.ExactCoins -> Unit
        }
    }
    return SendPlanDebugInfo.Coinage(strategyName = strategyName, details = details)
}

private fun ExternalPaymentPlan.toDebugInfo(): SendPlanDebugInfo.External = when (this) {
    is ExternalPaymentPlan.Ready -> SendPlanDebugInfo.External(
        strategyName = "Ready",
        details = listOf(
            "vouchers=${offboarding.vouchers.size}",
            "surplus=${offboarding.surplus.value}",
        ),
    )

    is ExternalPaymentPlan.LoadCoins -> SendPlanDebugInfo.External(
        strategyName = "LoadCoins",
        details = buildList {
            add("coinsToLoad=${coinsToLoad.size}")
            coinsToLoad.forEach { c ->
                add("  idx=${c.derivationIndex.item}  exp=2^${c.valueExponent.value}")
            }
            add("exactVouchers=${exactVouchers.size}")
        },
    )

    is ExternalPaymentPlan.NotEnoughAmount -> SendPlanDebugInfo.External(
        strategyName = "NotEnoughAmount",
        details = listOf(
            "activeVouchers=${activeVouchers.value}",
            "activeCoins=${activeCoins.value}",
            "deficit=${deficitToCoverWithCoins.value}",
        ),
    )
}
