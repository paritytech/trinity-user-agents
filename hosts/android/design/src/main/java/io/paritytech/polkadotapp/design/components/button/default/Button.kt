package io.paritytech.polkadotapp.design.components.button.default

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.LocalIndication
import androidx.compose.foundation.background
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.ripple
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonShape
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Check
import io.paritytech.polkadotapp.design.components.icon.vectors.SettingsFilled
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

private val ButtonIconSize = 16.dp
private val ButtonProgressSize = 16.dp
private val ButtonProgressStrokeWidth = 2.dp

@Composable
fun PolkadotButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    loading: Boolean = false,
    style: PolkadotButtonStyle = PolkadotButtonStyle.primary(),
    size: PolkadotButtonSize = PolkadotButtonSize.largeIncreased(),
    shape: Shape = PolkadotButtonShape.pill,
    interactionSource: MutableInteractionSource? = null,
    content: @Composable () -> Unit
) {
    val interactive = enabled && !loading

    PolkadotButtonInternal(
        onClick = onClick,
        modifier = modifier,
        enabled = interactive,
        shape = shape,
        containerBrush = style.colors.containerBrush(interactive),
        contentColor = style.colors.contentColor(interactive),
        rippleColor = style.rippleColor,
        contentPadding = size.padding,
        interactionSource = interactionSource,
        content = {
            Box {
                CompositionLocalProvider(LocalTextStyle provides size.textStyle) {
                    val progressAlpha by animateFloatAsState(
                        targetValue = if (loading) 1f else 0f,
                        label = "progress_alpha"
                    )

                    if (progressAlpha > 0f) {
                        Box(
                            modifier = Modifier
                                .align(Alignment.Center)
                                .alpha(progressAlpha)
                        ) {
                            CircularProgressIndicator(
                                modifier = Modifier.size(ButtonProgressSize),
                                strokeWidth = ButtonProgressStrokeWidth,
                                color = LocalContentColor.current
                            )
                        }
                    }

                    Box(modifier = Modifier.alpha(1f - progressAlpha)) {
                        content()
                    }
                }
            }
        }
    )
}

@Composable
fun PolkadotTextButton(
    text: String,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    loading: Boolean = false,
    style: PolkadotButtonStyle = PolkadotButtonStyle.primary(),
    size: PolkadotButtonSize = PolkadotButtonSize.largeIncreased(),
    shape: Shape = PolkadotButtonShape.pill,
    iconStart: ImageVector? = null,
    iconEnd: ImageVector? = null,
    interactionSource: MutableInteractionSource? = null,
    onClick: () -> Unit
) {
    PolkadotTextButtonInternal(
        modifier = modifier,
        enabled = enabled,
        loading = loading,
        style = style,
        size = size,
        shape = shape,
        iconStart = iconStart,
        iconEnd = iconEnd,
        interactionSource = interactionSource,
        onClick = onClick,
        label = { NovaText(text = text, maxLines = 1, overflow = TextOverflow.Ellipsis) }
    )
}

@Composable
fun PolkadotTextButton(
    text: AnnotatedString,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    loading: Boolean = false,
    style: PolkadotButtonStyle = PolkadotButtonStyle.primary(),
    size: PolkadotButtonSize = PolkadotButtonSize.largeIncreased(),
    shape: Shape = PolkadotButtonShape.pill,
    iconStart: ImageVector? = null,
    iconEnd: ImageVector? = null,
    interactionSource: MutableInteractionSource? = null,
    onClick: () -> Unit
) {
    PolkadotTextButtonInternal(
        modifier = modifier,
        enabled = enabled,
        loading = loading,
        style = style,
        size = size,
        shape = shape,
        iconStart = iconStart,
        iconEnd = iconEnd,
        interactionSource = interactionSource,
        onClick = onClick,
        label = { NovaText(text = text, maxLines = 1, overflow = TextOverflow.Ellipsis) }
    )
}

@Composable
private fun PolkadotTextButtonInternal(
    modifier: Modifier,
    enabled: Boolean,
    loading: Boolean,
    style: PolkadotButtonStyle,
    size: PolkadotButtonSize,
    shape: Shape,
    iconStart: ImageVector?,
    iconEnd: ImageVector?,
    interactionSource: MutableInteractionSource?,
    onClick: () -> Unit,
    label: @Composable () -> Unit
) {
    PolkadotButton(
        onClick = onClick,
        modifier = modifier,
        enabled = enabled,
        loading = loading,
        style = style,
        size = size,
        shape = shape,
        interactionSource = interactionSource
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            if (iconStart != null) {
                NovaIcon(
                    modifier = Modifier.size(ButtonIconSize),
                    imageVector = iconStart
                )

                HorizontalSpacer { small }
            }

            label()

            if (iconEnd != null) {
                HorizontalSpacer { small }

                NovaIcon(
                    modifier = Modifier.size(ButtonIconSize),
                    imageVector = iconEnd
                )
            }
        }
    }
}

@Composable
internal fun PolkadotButtonInternal(
    onClick: () -> Unit,
    modifier: Modifier,
    enabled: Boolean,
    shape: Shape,
    containerBrush: Brush,
    contentColor: Color,
    rippleColor: Color,
    contentPadding: PaddingValues,
    interactionSource: MutableInteractionSource?,
    border: BorderStroke? = null,
    content: @Composable RowScope.() -> Unit
) {
    val resolvedInteractionSource = interactionSource ?: remember { MutableInteractionSource() }

    CompositionLocalProvider(
        LocalIndication provides ripple(color = rippleColor)
    ) {
        PolkadotSurface(
            onClick = onClick,
            modifier = modifier.semantics { role = Role.Button },
            enabled = enabled,
            shape = shape,
            brush = containerBrush,
            contentColor = contentColor,
            border = border,
            interactionSource = resolvedInteractionSource
        ) {
            Row(
                modifier = Modifier.padding(contentPadding),
                horizontalArrangement = Arrangement.Center,
                verticalAlignment = Alignment.CenterVertically,
                content = content
            )
        }
    }
}

@Preview
@Composable
private fun PolkadotButtonPreview() {
    PolkadotTheme {
        val styles = listOf(
            "Primary" to PolkadotButtonStyle.primary(),
            "Secondary" to PolkadotButtonStyle.secondary(),
            "Tertiary" to PolkadotButtonStyle.tertiary(),
            "Ghost" to PolkadotButtonStyle.ghost(),
            "Destructive" to PolkadotButtonStyle.destructive()
        )

        Column(
            modifier = Modifier
                .background(PolkadotTheme.colors.bg.surface.main)
                .padding(PolkadotTheme.spacings.mediumIncreased)
        ) {
            styles.forEach { (label, style) ->
                Row(horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)) {
                    PolkadotTextButton(
                        text = label,
                        onClick = {},
                        style = style,
                        iconStart = NovaIcons.Check,
                        iconEnd = NovaIcons.SettingsFilled
                    )

                    PolkadotTextButton(
                        text = label,
                        onClick = {},
                        enabled = false,
                        style = style,
                        iconStart = NovaIcons.Check
                    )
                }

                VerticalSpacer { small }
            }
        }
    }
}
