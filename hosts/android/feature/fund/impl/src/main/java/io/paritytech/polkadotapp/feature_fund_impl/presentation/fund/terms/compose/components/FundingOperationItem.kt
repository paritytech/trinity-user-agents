package io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.R
import io.paritytech.polkadotapp.common.presentation.compose.withCurrencyTickerStyle
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Check
import io.paritytech.polkadotapp.design.components.icon.vectors.Info
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose.FundingOperation
import io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose.FundingOperation.Status
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalConversionFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.RoundPrecision

@Composable
fun FundingOperationItem(
    operation: FundingOperation,
) {
    val conversionFormatter = LocalConversionFormatter.current

    val bgColor = when (operation.status) {
        is Status.InProgress -> PolkadotTheme.colors.fg.warning
        is Status.Done -> PolkadotTheme.colors.fg.success
        is Status.Failure -> PolkadotTheme.colors.fg.error
    }.copy(alpha = 0.08f)

    val textColor = when (operation.status) {
        is Status.InProgress -> PolkadotTheme.colors.fg.primary
        is Status.Done -> PolkadotTheme.colors.fg.success
        is Status.Failure -> PolkadotTheme.colors.fg.error
    }

    val statusRes = when (operation.status) {
        is Status.InProgress -> R.string.fund_funding_pending
        is Status.Done -> R.string.fund_funding_completed
        is Status.Failure -> R.string.fund_funding_incomplete
    }

    Row {
        Column(modifier = Modifier.weight(1f)) {
            NovaText(
                text = stringResource(statusRes),
                style = PolkadotTheme.typography.body.medium,
                color = textColor
            )

            VerticalSpacer { tiny }

            NovaText(
                text = conversionFormatter.formatConversion(
                    tokenAmountFrom = operation.conversion.first,
                    tokenAmountTo = operation.conversion.second,
                    precisionFrom = RoundPrecision.DEFAULT,
                    precisionTo = RoundPrecision.HIGH,
                ).withCurrencyTickerStyle(PolkadotTheme.typography.body.large),
                style = PolkadotTheme.typography.body.large,
                color = PolkadotTheme.colors.fg.secondary
            )
        }

        when (val status = operation.status) {
            is Status.InProgress -> {
                CountdownTimer(
                    modifier = Modifier.size(40.dp),
                    duration = status.countdownTime
                )
            }

            is Status.Done -> {
                FundingStatusIcon(NovaIcons.Check, textColor, bgColor)
            }

            is Status.Failure -> {
                FundingStatusIcon(NovaIcons.Info, textColor, bgColor)
            }
        }
    }
}

@Composable
fun FundingStatusIcon(imageVector: ImageVector, tint: Color, bgColor: Color) {
    NovaIcon(
        modifier = Modifier
            .size(40.dp)
            .background(
                color = bgColor,
                shape = RoundedCornerShape(20.dp)
            )
            .padding(PolkadotTheme.spacings.small),
        imageVector = imageVector,
        tint = tint
    )
}
