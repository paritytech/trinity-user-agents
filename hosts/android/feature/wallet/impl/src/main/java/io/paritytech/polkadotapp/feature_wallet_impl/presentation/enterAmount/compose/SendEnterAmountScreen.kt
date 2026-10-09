package io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.compose

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.tooling.preview.Preview
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.compose.withCurrencyTickerStyle
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.validation.compose.rememberValidationActionHandle
import io.paritytech.polkadotapp.common.utils.CurrencyConfig
import io.paritytech.polkadotapp.common.utils.progressStallReport.StallReportContent
import io.paritytech.polkadotapp.common.utils.progressStallReport.previewStallReportOperations
import io.paritytech.polkadotapp.common.utils.progressStallReport.previewStallReportSteps
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonShape
import io.paritytech.polkadotapp.design.components.button.default.PolkadotButtonSize
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowUpwards
import io.paritytech.polkadotapp.design.components.progress.LoadingScreenState
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.configs.colors.AvatarColorScheme
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddress
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.formatFiatSigned
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.RoundPrecision
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.SendEnterAmountContract
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.SendEnterAmountUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.SendEnterAmountUiState.SendProgress
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.SendPlanDebugInfo
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.compose.components.EnterAmountBalance
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.compose.components.EnterAmountInput
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.compose.components.EnterAmountRecipient
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.compose.components.EnterAmountToolbar
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.domain.ConfirmGainingPrivacySpendDecision
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.domain.ConfirmGainingPrivacySpendUserAction
import kotlinx.coroutines.android.awaitFrame
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun SendEnterAmountScreen(contract: SendEnterAmountContract) {
    val state = contract.state.collectAsStateWithLifecycle().value

    PolkadotSurface {
        when (state) {
            is LoadingState.Loaded -> SendEnterAmountScreenInternal(
                state = state.data,
                // Hidden for now: diagnostics are still collected, just not shown.
                stallReport = {},
                onAmountChange = contract::onNewInput,
                onConfirmClick = contract::onConfirmClick,
                onBackClick = contract::onBackClick
            )

            else -> LoadingScreenState()
        }
    }

    GainingPrivacyConfirmationHost(contract)
}

@Composable
private fun GainingPrivacyConfirmationHost(contract: SendEnterAmountContract) {
    val handle = contract.sendValidationMixin
        .rememberValidationActionHandle<ConfirmGainingPrivacySpendUserAction, ConfirmGainingPrivacySpendDecision>()

    val action = handle.payload

    if (action != null) {
        SendConfirmGainingPrivacyBottomSheet(
            isVisible = handle.isVisible,
            action = action,
            onSendAnyway = { handle.respond(ConfirmGainingPrivacySpendDecision.SendAnyway) },
            onDismiss = { handle.respond(ConfirmGainingPrivacySpendDecision.Cancel) },
        )
    }
}

@Composable
private fun SendEnterAmountScreenInternal(
    state: SendEnterAmountUiState,
    stallReport: @Composable () -> Unit,
    onAmountChange: (String) -> Unit,
    onConfirmClick: () -> Unit,
    onBackClick: () -> Unit,
) {
    val formatter = LocalTokenAmountFormatter.current
    val focusRequester = remember { FocusRequester() }
    val keyboardController = LocalSoftwareKeyboardController.current

    LaunchedEffect(Unit) {
        // The amount field is composed during measure (BoxWithConstraints), so it exists only after the first frame
        awaitFrame()
        focusRequester.requestFocus()
        keyboardController?.show()
    }

    val symbol = remember(state.available) {
        formatter.formatToSymbol(state.available)
    }
    val amount = remember(state.spendable) {
        formatter.formatFiatSigned(state.spendable, withSymbol = true)
    }
    val gainingPrivacy = remember(state.gainingPrivacy) {
        state.gainingPrivacy?.let { formatter.formatTokenAmount(it, RoundPrecision.FIAT, withSymbol = false) }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .systemBarsPadding()
            .imePadding()
    ) {
        EnterAmountToolbar(onBackClick)

        VerticalSpacer { mediumIncreased }

        if (state.recipient != null) {
            EnterAmountRecipient(
                address = state.recipient,
                type = state.recipientType,
                avatarColor = state.recipientAvatarColor
            )
        }

        Column(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center
        ) {
            EnterAmountBalance(
                modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.large),
                amount = amount,
                gainingPrivacy = gainingPrivacy
            )

            VerticalSpacer { small }

            EnterAmountInput(
                modifier = Modifier.padding(
                    horizontal = PolkadotTheme.spacings.mediumIncreased
                ),
                input = state.input,
                fiatSymbol = CurrencyConfig.fiatSymbol,
                ticker = symbol,
                showError = state.showBalanceError,
                enabled = state.sendProgress is SendProgress.Idle && !state.isAmountLocked,
                focusRequester = focusRequester,
                onInputChange = onAmountChange
            )
        }

        state.debugPlanInfo?.let { DebugPlanInfo(it) }

        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = PolkadotTheme.spacings.large)
        ) {
            stallReport()
        }

        val progress = state.sendProgress
        PolkadotTextButton(
            modifier = Modifier
                .fillMaxWidth()
                .padding(PolkadotTheme.spacings.large),
            text = when (progress) {
                is SendProgress.Idle ->
                    if (state.isAmountPositive) {
                        stringResource(RCommon.string.send_enter_amount_send_button, state.input, symbol)
                            .withCurrencyTickerStyle(PolkadotTheme.typography.title.medium)
                    } else {
                        AnnotatedString(stringResource(RCommon.string.common_send))
                    }

                is SendProgress.Submitting -> AnnotatedString(stringResource(RCommon.string.send_enter_amount_submitting))
                is SendProgress.Settling -> AnnotatedString(stringResource(RCommon.string.send_enter_amount_settling))
            },
            size = PolkadotButtonSize.large(),
            onClick = onConfirmClick,
            enabled = state.isSendEnabled && progress is SendProgress.Idle,
            shape = PolkadotButtonShape.pill,
            iconStart = NovaIcons.ArrowUpwards
        )
    }
}

@Composable
private fun DebugPlanInfo(info: SendPlanDebugInfo) {
    val label = when (info) {
        is SendPlanDebugInfo.Coinage -> "Coinage"
        is SendPlanDebugInfo.External -> "External"
    }
    PolkadotSurface(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = PolkadotTheme.spacings.mediumIncreased, vertical = PolkadotTheme.spacings.small),
        shape = PolkadotTheme.shapes.small,
        color = Color(0x0FFFFFFF)
    ) {
        Column(modifier = Modifier.padding(PolkadotTheme.spacings.extraMedium)) {
            NovaText(text = "[DEBUG] $label: ${info.strategyName}", style = PolkadotTheme.typography.body.small)

            if (info.details.isNotEmpty()) {
                VerticalSpacer { tiny }
                info.details.forEach { line ->
                    NovaText(text = line, style = PolkadotTheme.typography.label.small)
                }
            }
        }
    }
}

/**
 * Stands in for the real [io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReport], which
 * only renders once the operation overruns its budget - something a preview never waits for.
 */
@Composable
private fun PreviewStallReport() {
    StallReportContent(
        runningOperations = previewStallReportOperations(),
        steps = previewStallReportSteps(),
    )
}

@Preview
@Composable
private fun SendEnterAmountScreenAllWidgetPreview() {
    CompositionLocalProvider(
        LocalTokenAmountFormatter provides TokenAmountFormatter.mocked,
        LocalPaymentAssetBrand provides PaymentAssetBrand.mocked
    ) {
        PolkadotTheme {
            SendEnterAmountScreenInternal(
                state = SendEnterAmountUiState(
                    input = "228.69",
                    recipient = "2o4ytihgkgrjbsk4kjb45lnqlkn35lk3ny73l54jnu45lkjulk5u4lu4lubhv",
                    recipientType = ExtractedAddress.DisplayType.ADDRESS,
                    available = TokenAmountModel.mock,
                    spendable = TokenAmountModel.mock(300),
                    gainingPrivacy = TokenAmountModel.mock(150),
                    sendProgress = SendProgress.Idle,
                    showBalanceError = true,
                    isAmountPositive = true,
                    isSendEnabled = false,
                    isAmountLocked = false,
                    recipientAvatarColor = AvatarColorScheme.Garnet
                ),
                stallReport = { PreviewStallReport() },
                onAmountChange = {},
                onConfirmClick = {},
                onBackClick = {}
            )
        }
    }
}
