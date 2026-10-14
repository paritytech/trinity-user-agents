package io.paritytech.polkadotapp.feature_wallet_impl.presentation.transactionResult.compose

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.InlineTextContent
import androidx.compose.foundation.text.appendInlineContent
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.Placeholder
import androidx.compose.ui.text.PlaceholderVerticalAlign
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.sp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.CheckCircleFilled
import io.paritytech.polkadotapp.design.components.icon.vectors.Clock
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.transactionResult.TransactionSuccessUiState.Finality
import kotlinx.collections.immutable.persistentMapOf
import io.paritytech.polkadotapp.common.R as RCommon

private const val ICON_ID = "finalityIcon"
private val ICON_SIZE = 16.sp
private val ICON_START_PADDING = 4.sp

@Composable
fun PaymentFinalityLabel(
    modifier: Modifier = Modifier,
    finality: Finality,
) {
    val label = stringResource(finality.textRes())
    // A non-breaking space keeps the icon on the same line as the last word.
    val text = remember(label) {
        buildAnnotatedString {
            append(label)
            append('\u00A0')
            appendInlineContent(ICON_ID)
        }
    }
    val inlineContent = persistentMapOf(
        ICON_ID to InlineTextContent(
            placeholder = Placeholder(
                width = (ICON_SIZE.value + ICON_START_PADDING.value).sp,
                height = ICON_SIZE,
                placeholderVerticalAlign = PlaceholderVerticalAlign.TextCenter
            )
        ) {
            FinalityIcon(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(start = with(LocalDensity.current) { ICON_START_PADDING.toDp() }),
                finality = finality
            )
        }
    )

    NovaText(
        modifier = modifier,
        text = text,
        style = PolkadotTheme.typography.body.medium,
        textAlign = TextAlign.Center,
        color = PolkadotTheme.colors.fg.secondary,
        inlineContent = inlineContent
    )
}

@Composable
private fun FinalityIcon(
    modifier: Modifier,
    finality: Finality,
) {
    when (finality) {
        Finality.PENDING -> NovaIcon(
            modifier = modifier,
            imageVector = NovaIcons.Clock,
            tint = PolkadotTheme.colors.fg.secondary
        )

        Finality.CONFIRMED -> NovaIcon(
            modifier = modifier,
            imageVector = NovaIcons.CheckCircleFilled,
            tint = PolkadotTheme.colors.fg.success
        )
    }
}

private fun Finality.textRes(): Int = when (this) {
    Finality.PENDING -> RCommon.string.transaction_success_finality_pending
    Finality.CONFIRMED -> RCommon.string.transaction_success_finality_confirmed
}

@Preview
@Composable
private fun PaymentFinalityPendingLabelPreview() {
    PolkadotTheme {
        PaymentFinalityLabel(finality = Finality.PENDING)
    }
}

@Preview
@Composable
private fun PaymentFinalityConfirmedLabelPreview() {
    PolkadotTheme {
        PaymentFinalityLabel(finality = Finality.CONFIRMED)
    }
}
