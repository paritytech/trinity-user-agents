package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
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

/** A screen title with an optional back button on the left and actions on the right. */
@Composable
fun FundingHeader(
    title: String,
    onBack: (() -> Unit)?,
    modifier: Modifier = Modifier,
    trailing: @Composable RowScope.() -> Unit = {},
) {
    Row(
        modifier = modifier.fillMaxWidth(),
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
    icon: ImageVector,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
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
    label: String,
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
    value: @Composable RowScope.() -> Unit,
) {
    PolkadotSurface(
        modifier = modifier.fillMaxWidth(),
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
        modifier = modifier.fillMaxWidth(),
        shape = PolkadotTheme.shapes.large,
        color = PolkadotTheme.colors.bg.surface.main,
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(CARD_DIVIDER)) {
            content()
        }
    }
}

/** A round monogram standing in for a network or token logo. */
@Composable
fun FundingMonogram(
    text: String,
    modifier: Modifier = Modifier,
) {
    PolkadotSurface(
        modifier = modifier.size(MONOGRAM_SIZE),
        shape = PolkadotTheme.shapes.full,
        color = PolkadotTheme.colors.bg.surface.nested,
        contentAlignment = Alignment.Center,
    ) {
        NovaText(
            text = text,
            style = PolkadotTheme.typography.title.small,
            color = PolkadotTheme.colors.fg.primary,
        )
    }
}

private val CIRCLE_BUTTON_SIZE = 40.dp
private val CHEVRON_SIZE = 16.dp
private val CARD_DIVIDER = 2.dp
private val MONOGRAM_SIZE = 40.dp
