package io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose.components

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.HorizontalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.R
import io.paritytech.polkadotapp.common.presentation.formatters.time.LocalTimeFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.TimeFormatter
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Info
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose.FundingOperation
import io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose.FundingOperation.Status
import io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose.getStatus
import io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose.mockOperation
import io.paritytech.polkadotapp.feature_prices_api.presentation.formatter.FiatFormatter
import io.paritytech.polkadotapp.feature_prices_api.presentation.formatter.LocalFiatFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.ConversionFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.KnownTokenFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalConversionFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalKnownTokenFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf
import kotlin.time.Duration.Companion.seconds

@Composable
fun FundingOperations(
    modifier: Modifier,
    operations: ImmutableList<FundingOperation>,
) {
    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.large,
        color = PolkadotTheme.colors.bg.surface.container
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(PolkadotTheme.spacings.large),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            val status = operations.getStatus()

            Status(status)

            VerticalSpacer { large }

            operations.forEachIndexed { index, operation ->
                FundingOperationItem(operation)

                if (index != operations.lastIndex)
                    FundingDivider()
            }

            VerticalSpacer { large }

            if (status.shouldShowWarning()) {
                Warning(status)
            } else {
                NovaText(
                    modifier = Modifier.fillMaxWidth(),
                    text = stringResource(R.string.fund_funding_failure),
                    style = PolkadotTheme.typography.body.medium,
                    color = PolkadotTheme.colors.fg.error,
                )
            }
        }
    }
}

private fun Status.shouldShowWarning(): Boolean {
    return this is Status.InProgress || this is Status.Done
}

@Composable
private fun Warning(status: Status) {
    val color = when (status) {
        is Status.InProgress -> PolkadotTheme.colors.fg.warning
        is Status.Done -> PolkadotTheme.colors.fg.success
        is Status.Failure -> PolkadotTheme.colors.fg.error
    }

    PolkadotSurface(
        modifier = Modifier.fillMaxWidth(),
        shape = PolkadotTheme.shapes.large,
        color = color.copy(alpha = 0.08f)
    ) {
        Row(
            modifier = Modifier.padding(PolkadotTheme.spacings.mediumIncreased),
            verticalAlignment = Alignment.CenterVertically
        ) {
            val text = stringResource(
                when (status) {
                    is Status.InProgress -> R.string.fund_funding_warning
                    is Status.Done -> R.string.fund_funding_success
                    is Status.Failure -> R.string.fund_funding_failure
                }
            )

            NovaText(
                modifier = Modifier.weight(1f),
                text = text,
                style = PolkadotTheme.typography.body.medium,
                color = color,
            )

            if (status is Status.InProgress) {
                NovaIcon(
                    modifier = Modifier.size(28.dp),
                    imageVector = NovaIcons.Info,
                    tint = color,
                )
            }
        }
    }
}

@Composable
private fun Status(status: Status) {
    val statusRes = when (status) {
        is Status.InProgress -> R.string.fund_funding_pending_title
        is Status.Done -> R.string.fund_funding_complete_title
        is Status.Failure -> R.string.fund_funding_failed_title
    }

    NovaText(
        modifier = Modifier.fillMaxWidth(),
        text = stringResource(statusRes),
        style = PolkadotTheme.typography.title.large,
        color = PolkadotTheme.colors.fg.primary,
    )
}

@Composable
private fun FundingDivider() {
    VerticalSpacer { extraMedium }

    HorizontalDivider(
        thickness = 1.dp,
        color = Color(0x1FFFFFFF)
    )

    VerticalSpacer { extraMedium }
}

@Preview
@Composable
private fun FundingOperationsPreview() {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalTokenAmountFormatter provides TokenAmountFormatter.mocked,
            LocalPaymentAssetBrand provides PaymentAssetBrand.mocked,
            LocalConversionFormatter provides ConversionFormatter.mocked,
            LocalKnownTokenFormatter provides KnownTokenFormatter.mocked,
            LocalTimeFormatter provides TimeFormatter.mocked(LocalContext.current),
            LocalFiatFormatter provides FiatFormatter.mocked
        ) {
            FundingOperations(
                modifier = Modifier.fillMaxWidth(),
                operations = persistentListOf(
                    mockOperation(),
                    mockOperation().copy(status = Status.Failure),
                    mockOperation().copy(status = Status.InProgress(5.seconds)),
                )
            )
        }
    }
}
