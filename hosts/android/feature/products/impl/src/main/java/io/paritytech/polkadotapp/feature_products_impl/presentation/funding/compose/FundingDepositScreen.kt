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
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingDepositContent
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingDepositUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingCircleButton
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingHeader
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
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased),
    ) {
        FundingHeader(
            title = stringResource(if (state.isBank) RCommon.string.funding_summary_bank_title else RCommon.string.funding_summary_crypto_title),
            onBack = onBack,
        )

        when (val content = state.content) {
            is FundingDepositContent.Crypto -> CryptoDeposit(content = content, onCopy = onCopy)
            is FundingDepositContent.Bank -> BankDeposit(content = content, onCopy = onCopy)
            FundingDepositContent.NoProvider -> NovaText(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = PolkadotTheme.spacings.large),
                text = stringResource(RCommon.string.funding_error_no_provider),
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
                        if (content.preparing) RCommon.string.funding_deposit_preparing else RCommon.string.funding_deposit_finding_provider,
                    ),
                    style = PolkadotTheme.typography.body.medium,
                    color = PolkadotTheme.colors.fg.secondary,
                )
            }
        }

        if (state.started) {
            Row(horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)) {
                PolkadotTextButton(
                    modifier = Modifier.weight(1f),
                    text = stringResource(RCommon.string.common_cancel),
                    style = PolkadotButtonStyle.destructive(),
                    onClick = onCancel,
                )
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

/** Asks before giving up on a deposit the user may already have paid. */
@Composable
fun FundingCancelConfirmScreen(
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
            text = stringResource(RCommon.string.funding_cancel_title),
            style = PolkadotTheme.typography.headline.medium,
            color = PolkadotTheme.colors.fg.primary,
        )
        NovaText(
            text = stringResource(RCommon.string.funding_cancel_body),
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = TextAlign.Center,
        )
        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.common_cancel),
            style = PolkadotButtonStyle.destructive(),
            onClick = onConfirm,
        )
        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.funding_cancel_keep),
            onClick = onBack,
        )
    }
}
