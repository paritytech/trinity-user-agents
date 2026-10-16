package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ContentCopy
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.qr.QrCode
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingCancelUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingDepositContent
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingDepositUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingCircleButton
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingHeader
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.fundingProblemText
import kotlinx.coroutines.delay
import uniffi.truapi.FundingRail
import io.paritytech.polkadotapp.common.R as RCommon

private val QR_SIZE = 220.dp
private val WAITING_HEIGHT = 52.dp
private val SMALL_INDICATOR_SIZE = 16.dp
private val INDICATOR_SIZE = 32.dp
private val INDICATOR_STROKE = 2.dp

/** Where to send the funds, as the provider reported it, while the session waits for them. */
@Composable
fun FundingDepositScreen(
    state: FundingDepositUiState,
    onBack: () -> Unit,
    onCopy: (String) -> Unit,
    onCancel: () -> Unit,
    onReviewOffer: () -> Unit,
    onClose: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased),
    ) {
        FundingHeader(
            title = stringResource(
                when (state.rail) {
                    FundingRail.CARD -> RCommon.string.funding_summary_card_title
                    FundingRail.BANK -> RCommon.string.funding_summary_bank_title
                    FundingRail.CRYPTO -> RCommon.string.funding_summary_crypto_title
                },
            ),
            onBack = onBack,
        )

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
                text = stringResource(RCommon.string.funding_failure_close),
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
                WaitingPill(modifier = Modifier.weight(1f))
            }
        }
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
                shown = content.shortAddress,
                onCopy = onCopy,
            )
        }
    }
}

@Composable
private fun BankDeposit(content: FundingDepositContent.Bank, onCopy: (String) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)) {
        CopyRow(stringResource(RCommon.string.funding_deposit_exact_amount), content.amount, content.amount, onCopy)
        content.beneficiary?.let { CopyRow(stringResource(RCommon.string.funding_deposit_beneficiary), it, it, onCopy) }
        content.account?.let { CopyRow(stringResource(RCommon.string.funding_deposit_account), it, it, onCopy) }
        content.bankCode?.let { CopyRow(stringResource(RCommon.string.funding_deposit_bank_code), it, it, onCopy) }
        CopyRow(stringResource(RCommon.string.funding_deposit_reference), content.reference, content.reference, onCopy)
    }
}

@Composable
private fun CopyRow(title: String, value: String, shown: String, onCopy: (String) -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = PolkadotTheme.spacings.small),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f)) {
            NovaText(
                text = title,
                style = PolkadotTheme.typography.caption.medium,
                color = PolkadotTheme.colors.fg.secondary,
            )
            NovaText(
                text = shown,
                style = PolkadotTheme.typography.body.medium,
                color = PolkadotTheme.colors.fg.primary,
            )
        }
        FundingCircleButton(icon = NovaIcons.ContentCopy, onClick = { onCopy(value) })
    }
}

@Composable
private fun WaitingPill(modifier: Modifier = Modifier) {
    PolkadotSurface(
        modifier = modifier.height(WAITING_HEIGHT),
        shape = PolkadotTheme.shapes.full,
        color = PolkadotTheme.colors.bg.surface.nested,
        contentAlignment = Alignment.Center,
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraSmall),
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

/** Where the user's cancel stands, once money has moved and the provider decides it. */
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

/** The provider's terms for cancelling: what it keeps and what comes back, for the user to accept or decline. */
@Composable
fun FundingCancelOfferScreen(
    offer: FundingCancelUiState.Offered,
    isAnswering: Boolean,
    onBack: () -> Unit,
    onAccept: () -> Unit,
    onDecline: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.medium),
    ) {
        FundingHeader(title = "", onBack = onBack)
        NovaText(
            text = stringResource(RCommon.string.funding_cancel_offer_title),
            style = PolkadotTheme.typography.headline.medium,
            color = PolkadotTheme.colors.fg.primary,
        )
        NovaText(
            text = stringResource(RCommon.string.funding_cancel_offer_body),
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = TextAlign.Center,
        )
        Column(modifier = Modifier.fillMaxWidth()) {
            OfferRow(stringResource(RCommon.string.funding_cancel_offer_fee), offer.fee)
            OfferRow(stringResource(RCommon.string.funding_cancel_offer_refund), offer.refund)
        }
        val secondsLeft = offerSecondsLeft(offer)
        val lapsed = secondsLeft == 0L
        secondsLeft?.let { seconds ->
            NovaText(
                text = if (lapsed) {
                    stringResource(RCommon.string.funding_cancel_offer_expired)
                } else {
                    stringResource(RCommon.string.funding_cancel_offer_valid_for, "%d:%02d".format(seconds / 60, seconds % 60))
                },
                style = PolkadotTheme.typography.body.small,
                color = PolkadotTheme.colors.fg.secondary,
            )
        }
        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.funding_cancel_offer_accept),
            style = PolkadotButtonStyle.destructive(),
            loading = isAnswering,
            enabled = !lapsed,
            onClick = onAccept,
        )
        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.funding_cancel_offer_keep),
            enabled = !isAnswering,
            onClick = onDecline,
        )
    }
}

/** The seconds left on the offer, ticking down once a second; `null` when it has no deadline. */
@Composable
private fun offerSecondsLeft(offer: FundingCancelUiState.Offered): Long? {
    val expiresAtMs = offer.expiresAtMs ?: return null
    val left by produceState(initialValue = offer.secondsLeft(System.currentTimeMillis()), expiresAtMs) {
        while (true) {
            value = offer.secondsLeft(System.currentTimeMillis())
            if (value == 0L) break
            delay(OFFER_TICK_MS)
        }
    }
    return left
}

private const val OFFER_TICK_MS = 1_000L

@Composable
private fun OfferRow(title: String, value: String) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = PolkadotTheme.spacings.small),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        NovaText(
            modifier = Modifier.weight(1f),
            text = title,
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.secondary,
        )
        NovaText(text = value, style = PolkadotTheme.typography.body.mediumEmphasized, color = PolkadotTheme.colors.fg.primary)
    }
}

/**
 * Asks before cancelling. A crypto top-up's deposit address stops working; once money has moved, the provider decides
 * whether it can still cancel, and may charge a fee.
 */
@Composable
fun FundingCancelConfirmScreen(
    state: FundingDepositUiState,
    onBack: () -> Unit,
    onConfirm: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.medium),
    ) {
        FundingHeader(title = "", onBack = onBack)
        NovaText(
            text = stringResource(if (state.withdraw) RCommon.string.funding_cancel_withdrawal_title else RCommon.string.funding_cancel_title),
            style = PolkadotTheme.typography.headline.medium,
            color = PolkadotTheme.colors.fg.primary,
        )
        NovaText(
            text = stringResource(
                if (state.content is FundingDepositContent.Crypto) RCommon.string.funding_cancel_body else RCommon.string.funding_cancel_body_provider,
            ),
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = TextAlign.Center,
        )
        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.common_cancel),
            style = PolkadotButtonStyle.destructive(),
            loading = state.isCancelling,
            onClick = onConfirm,
        )
        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.funding_cancel_keep),
            onClick = onBack,
        )
    }
}
