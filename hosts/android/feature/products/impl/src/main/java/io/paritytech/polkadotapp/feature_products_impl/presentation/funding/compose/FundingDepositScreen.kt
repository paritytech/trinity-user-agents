package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ContentCopy
import io.paritytech.polkadotapp.design.components.icon.vectors.SyncArrowsOutlined
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.qr.QrCode
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingCancelUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingCryptoIcon
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingDepositContent
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingDepositUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingLiveProgressUi
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingCircleButton
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingCryptoIconImage
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingHeader
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingLiveLine
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingTimelineCard
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.fundingProblemText
import uniffi.truapi.FundingRail
import kotlin.time.Instant
import io.paritytech.polkadotapp.common.R as RCommon

private val QR_SIZE = 220.dp
private val WAITING_HEIGHT = 52.dp
private val SMALL_INDICATOR_SIZE = 16.dp
private val INDICATOR_SIZE = 32.dp
private val INDICATOR_STROKE = 2.dp
private val LIVE_ICON_SIZE = 48.dp
private val LIVE_GLYPH_SIZE = 24.dp
private val COPY_ROW_ICON_SIZE = 20.dp

@Composable
fun FundingDepositScreen(
    modifier: Modifier = Modifier,
    state: FundingDepositUiState,
    onBack: () -> Unit,
    onCopy: (String) -> Unit,
    onCancel: () -> Unit,
    onReviewOffer: () -> Unit,
    onClose: () -> Unit,
) {
    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased),
    ) {
        FundingHeader(
            modifier = Modifier.fillMaxWidth(),
            title = if (state.progress != null) "" else stringResource(
                when (state.rail) {
                    FundingRail.CARD -> RCommon.string.funding_summary_card_title
                    FundingRail.BANK -> RCommon.string.funding_summary_bank_title
                    FundingRail.CRYPTO -> RCommon.string.funding_summary_crypto_title
                },
            ),
            onBack = onBack,
        )

        state.progress?.let { LiveProgress(figure = state.figure, progress = it, now = state.now) }

        when (val content = state.content) {
            is FundingDepositContent.Crypto -> CryptoDeposit(content = content, onCopy = onCopy)
            is FundingDepositContent.Bank -> BankDeposit(content = content, onCopy = onCopy)
            is FundingDepositContent.Problem -> NovaText(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = PolkadotTheme.spacings.large),
                text = fundingProblemText(content.problem),
                style = PolkadotTheme.typography.body.medium,
                color = PolkadotTheme.colors.fg.error,
                textAlign = TextAlign.Center,
            )

            FundingDepositContent.ProgressOnly -> if (state.progress == null) {
                NovaCircularProgressIndicator(
                    modifier = Modifier
                        .padding(top = PolkadotTheme.spacings.extraLarge)
                        .size(INDICATOR_SIZE)
                        .align(Alignment.CenterHorizontally),
                )
            }

            is FundingDepositContent.Waiting -> Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = PolkadotTheme.spacings.extraLarge),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.medium),
            ) {
                NovaCircularProgressIndicator(modifier = Modifier.size(INDICATOR_SIZE))
                NovaText(
                    text = stringResource(
                        when {
                            !content.preparing -> RCommon.string.funding_deposit_finding_provider
                            content.card -> RCommon.string.funding_deposit_card_in_progress
                            else -> RCommon.string.funding_deposit_preparing
                        },
                    ),
                    style = PolkadotTheme.typography.body.medium,
                    color = PolkadotTheme.colors.fg.secondary,
                )
            }
        }

        state.cancel?.let { CancelStatus(cancel = it, withdraw = state.withdraw, onReviewOffer = onReviewOffer) }

        if (state.failed) {
            PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.common_close),
                onClick = onClose,
            )
        } else if (state.started) {
            Row(horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)) {
                if (state.canCancel) {
                    PolkadotTextButton(
                        modifier = Modifier.weight(1f),
                        text = stringResource(
                            if (state.isSecondCancel) RCommon.string.funding_cancel_anyway else RCommon.string.common_cancel,
                        ),
                        style = PolkadotButtonStyle.destructive(),
                        onClick = onCancel,
                    )
                }
                if (state.progress == null) WaitingPill(modifier = Modifier.weight(1f).height(WAITING_HEIGHT))
            }
        }
    }
}

@Composable
private fun LiveProgress(figure: String, progress: FundingLiveProgressUi, now: Instant) {
    Column(
        modifier = Modifier.fillMaxWidth(),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
    ) {
        PolkadotSurface(
            modifier = Modifier.size(LIVE_ICON_SIZE),
            shape = PolkadotTheme.shapes.full,
            color = PolkadotTheme.colors.bg.surface.nested,
            contentAlignment = Alignment.Center,
        ) {
            NovaIcon(modifier = Modifier.size(LIVE_GLYPH_SIZE), imageVector = NovaIcons.SyncArrowsOutlined)
        }
        NovaText(
            text = figure,
            style = PolkadotTheme.typography.headline.large,
            color = PolkadotTheme.colors.fg.primary,
            maxLines = 1,
        )
        VerticalSpacer { small }
        FundingTimelineCard(
            modifier = Modifier.fillMaxWidth(),
            timeline = progress.timeline,
            band = progress.band?.let { band -> band.arg?.let { stringResource(band.text, it) } ?: stringResource(band.text) },
            bandWarning = progress.band?.warning == true,
        )
        FundingLiveLine(
            modifier = Modifier.fillMaxWidth(),
            detail = progress.detail,
            lastHeardAt = progress.lastHeardAt,
            now = now,
        )
    }
}

@Composable
private fun CryptoDeposit(content: FundingDepositContent.Crypto, onCopy: (String) -> Unit) {
    Column(
        modifier = Modifier.fillMaxWidth(),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased),
    ) {
        PolkadotSurface(
            shape = PolkadotTheme.shapes.large,
            color = PolkadotTheme.colors.fg.staticWhite,
        ) {
            QrCode(
                modifier = Modifier
                    .padding(PolkadotTheme.spacings.medium)
                    .size(QR_SIZE),
                text = content.qrPayload,
            )
        }

        Column(verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)) {
            CopyRow(
                title = stringResource(if (content.exact) RCommon.string.funding_deposit_exact_amount else RCommon.string.funding_deposit_at_least),
                value = content.amount,
                shown = content.amount,
                onCopy = onCopy,
            )
            CopyRow(
                title = stringResource(RCommon.string.funding_deposit_address_on, content.networkName),
                value = content.address,
                shown = content.address,
                onCopy = onCopy,
                icon = content.networkIcon,
            )
        }
    }
}

@Composable
private fun BankDeposit(content: FundingDepositContent.Bank, onCopy: (String) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)) {
        CopyRow(stringResource(RCommon.string.funding_row_send_exact), content.amount, content.amount, onCopy)
        content.beneficiary?.let { CopyRow(stringResource(RCommon.string.funding_deposit_beneficiary), it, it, onCopy) }
        content.account?.let { CopyRow(stringResource(RCommon.string.funding_deposit_account), it, it, onCopy) }
        content.bankCode?.let { CopyRow(stringResource(RCommon.string.funding_deposit_bank_code), it, it, onCopy) }
        CopyRow(stringResource(RCommon.string.funding_deposit_reference), content.reference, content.reference, onCopy)
        ArrivesRow()
    }
}

@Composable
private fun ArrivesRow() {
    Row(modifier = Modifier.fillMaxWidth().padding(vertical = PolkadotTheme.spacings.small)) {
        NovaText(
            modifier = Modifier.weight(1f),
            text = stringResource(RCommon.string.funding_row_arrives),
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.primary,
        )
        Column(horizontalAlignment = Alignment.End) {
            NovaText(
                text = stringResource(RCommon.string.funding_row_arrives_bank),
                style = PolkadotTheme.typography.body.mediumEmphasized,
                color = PolkadotTheme.colors.fg.primary,
            )
            NovaText(
                text = stringResource(RCommon.string.funding_row_rate_note),
                style = PolkadotTheme.typography.caption.medium,
                color = PolkadotTheme.colors.fg.secondary,
            )
        }
    }
}

@Composable
private fun CopyRow(
    title: String,
    value: String,
    shown: String,
    onCopy: (String) -> Unit,
    icon: FundingCryptoIcon? = null,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = PolkadotTheme.spacings.small),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
    ) {
        icon?.let { FundingCryptoIconImage(modifier = Modifier.size(COPY_ROW_ICON_SIZE), icon = it, monogram = false) }
        NovaText(
            modifier = Modifier.weight(1f),
            text = title,
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.primary,
        )
        NovaText(
            modifier = Modifier.weight(1f, fill = false),
            text = shown,
            style = PolkadotTheme.typography.body.mediumEmphasized,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = TextAlign.End,
            maxLines = 1,
            overflow = TextOverflow.MiddleEllipsis,
        )
        FundingCircleButton(icon = NovaIcons.ContentCopy, onClick = { onCopy(value) })
    }
}

@Composable
private fun WaitingPill(modifier: Modifier = Modifier) {
    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.full,
        color = PolkadotTheme.colors.bg.surface.nested,
        contentAlignment = Alignment.Center,
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraSmall, Alignment.CenterHorizontally),
        ) {
            NovaCircularProgressIndicator(modifier = Modifier.size(SMALL_INDICATOR_SIZE), strokeWidth = INDICATOR_STROKE)
            NovaText(
                text = stringResource(RCommon.string.funding_deposit_waiting),
                style = PolkadotTheme.typography.body.mediumEmphasized,
                color = PolkadotTheme.colors.fg.primary,
            )
        }
    }
}

@Composable
private fun CancelStatus(cancel: FundingCancelUiState, withdraw: Boolean, onReviewOffer: () -> Unit) {
    Column(
        modifier = Modifier.fillMaxWidth(),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
    ) {
        when (cancel) {
            FundingCancelUiState.Requested -> PendingLine(stringResource(RCommon.string.funding_cancel_requested))
            FundingCancelUiState.Cancelling -> PendingLine(stringResource(RCommon.string.funding_cancel_accepted))
            is FundingCancelUiState.Refused -> NovaText(
                text = stringResource(
                    if (withdraw) RCommon.string.funding_cancel_refused_withdrawal else RCommon.string.funding_cancel_refused_top_up,
                    cancel.reason,
                ),
                style = PolkadotTheme.typography.body.medium,
                color = PolkadotTheme.colors.fg.secondary,
                textAlign = TextAlign.Center,
            )

            is FundingCancelUiState.Offered -> {
                NovaText(
                    text = stringResource(RCommon.string.funding_cancel_offer_waiting),
                    style = PolkadotTheme.typography.body.medium,
                    color = PolkadotTheme.colors.fg.secondary,
                    textAlign = TextAlign.Center,
                )
                PolkadotTextButton(
                    modifier = Modifier.fillMaxWidth(),
                    text = stringResource(RCommon.string.funding_cancel_offer_review),
                    onClick = onReviewOffer,
                )
            }
        }
    }
}

@Composable
private fun PendingLine(text: String) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraSmall),
    ) {
        NovaCircularProgressIndicator(modifier = Modifier.size(SMALL_INDICATOR_SIZE), strokeWidth = INDICATOR_STROKE)
        NovaText(text = text, style = PolkadotTheme.typography.body.medium, color = PolkadotTheme.colors.fg.secondary)
    }
}
