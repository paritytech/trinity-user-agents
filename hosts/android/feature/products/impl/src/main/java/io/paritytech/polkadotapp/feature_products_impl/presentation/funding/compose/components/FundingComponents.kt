package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import coil.compose.AsyncImagePainter
import coil.compose.rememberAsyncImagePainter
import coil.request.ImageRequest
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButtonSize
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowLeft
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowRight
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_api.presentation.funding.failedText
import io.paritytech.polkadotapp.feature_products_api.presentation.funding.text
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingCryptoIcon
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingSummaryProblem
import io.paritytech.polkadotapp.common.R as RCommon

/** A screen title with an optional back button on the left and actions on the right. */
@Composable
fun FundingHeader(
    modifier: Modifier = Modifier,
    title: String,
    onBack: (() -> Unit)?,
    trailing: @Composable RowScope.() -> Unit = {},
) {
    Row(
        modifier = modifier,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (onBack != null) {
            FundingCircleButton(icon = NovaIcons.ArrowLeft, onClick = onBack)
            HorizontalSpacer { small }
        }

        NovaText(
            modifier = Modifier.weight(1f),
            text = title,
            style = PolkadotTheme.typography.title.large,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = if (onBack == null) TextAlign.Start else TextAlign.Center,
            maxLines = 1,
        )

        if (onBack != null) {
            Box(modifier = Modifier.size(CIRCLE_BUTTON_SIZE))
        }
        trailing()
    }
}

@Composable
fun FundingCircleButton(
    modifier: Modifier = Modifier,
    icon: ImageVector,
    onClick: () -> Unit,
) {
    PolkadotIconButton(
        modifier = modifier,
        icon = icon,
        onClick = onClick,
        style = PolkadotButtonStyle.secondary(),
        size = PolkadotIconButtonSize.medium(),
    )
}

/** A row of a summary card: a label on the left, a value and an optional chevron on the right. */
@Composable
fun FundingInfoRow(
    modifier: Modifier = Modifier,
    label: String,
    onClick: (() -> Unit)? = null,
    value: @Composable RowScope.() -> Unit,
) {
    PolkadotSurface(
        modifier = modifier,
        color = PolkadotTheme.colors.bg.surface.container,
        enabled = onClick != null,
        onClick = onClick,
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(
                    horizontal = PolkadotTheme.spacings.mediumIncreased,
                    vertical = PolkadotTheme.spacings.extraMedium,
                ),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
        ) {
            NovaText(
                modifier = Modifier.weight(1f),
                text = label,
                style = PolkadotTheme.typography.body.medium,
                color = PolkadotTheme.colors.fg.secondary,
            )
            value()
            if (onClick != null) {
                NovaIcon(
                    modifier = Modifier.size(CHEVRON_SIZE),
                    imageVector = NovaIcons.ArrowRight,
                    tint = PolkadotTheme.colors.fg.tertiary,
                )
            }
        }
    }
}

/** Rows that read as one card, rounded at its ends. */
@Composable
fun FundingCard(
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.large,
        color = PolkadotTheme.colors.bg.surface.main,
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(CARD_DIVIDER)) {
            content()
        }
    }
}

/**
 * A crypto network's or token's icon from its provider's worker archive, or its first letter while that loads and when
 * there is none. Without [monogram] nothing is drawn until the icon loads.
 */
@Composable
fun FundingCryptoIconImage(
    modifier: Modifier = Modifier,
    icon: FundingCryptoIcon,
    monogram: Boolean = true,
) {
    val context = LocalContext.current
    val request = remember(icon.image) { ImageRequest.Builder(context).data(icon.image).size(FUNDING_ICON_PIXELS).build() }
    // Sized up front, so the load starts though nothing is drawn until it succeeds.
    val painter = rememberAsyncImagePainter(request)

    if (painter.state is AsyncImagePainter.State.Success) {
        Image(modifier = modifier, painter = painter, contentDescription = null)
    } else if (monogram) {
        PolkadotSurface(
            modifier = modifier,
            shape = PolkadotTheme.shapes.full,
            color = PolkadotTheme.colors.bg.surface.nested,
            contentAlignment = Alignment.Center,
        ) {
            NovaText(
                modifier = Modifier.wrapContentSize(),
                text = icon.monogram,
                style = PolkadotTheme.typography.title.small,
                color = PolkadotTheme.colors.fg.primary,
            )
        }
    }
}

private val CIRCLE_BUTTON_SIZE = 40.dp
private val CHEVRON_SIZE = 16.dp
private val CARD_DIVIDER = 2.dp
val FUNDING_CRYPTO_ICON_SIZE = 40.dp

// Providers ship each icon as a 96×96 PNG.
private const val FUNDING_ICON_PIXELS = 96

/** What stands in the way, in the user's words. */
@Composable
fun fundingProblemText(problem: FundingSummaryProblem): String = when (problem) {
    FundingSummaryProblem.StartFailed -> stringResource(RCommon.string.funding_error_start_failed)
    FundingSummaryProblem.NoProvider -> stringResource(RCommon.string.funding_error_no_provider)
    FundingSummaryProblem.QuotesFailed -> stringResource(RCommon.string.funding_error_quotes_failed)
    is FundingSummaryProblem.QuoteRefused -> problem.text.text()
    is FundingSummaryProblem.Failed -> problem.reason.failedText()
    is FundingSummaryProblem.Minimum -> stringResource(RCommon.string.funding_error_minimum, problem.amount)
    is FundingSummaryProblem.Maximum -> stringResource(RCommon.string.funding_error_maximum, problem.amount)
}
