@file:OptIn(ExperimentalMaterial3Api::class)

package io.paritytech.polkadotapp.design.components.dialog

import androidx.compose.foundation.LocalIndication
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.BasicAlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ripple
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.DialogProperties
import io.paritytech.polkadotapp.design.colors.LegacyNovaStableColors
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun NovaAlertDialog(
    modifier: Modifier = Modifier,
    text: String,
    positiveButtonTitle: String,
    onPositiveButtonClick: () -> Unit,
    onDismissRequest: () -> Unit,
    negativeButtonTitle: String? = null,
    onNegativeButtonClick: (() -> Unit)? = null,
    title: String? = null,
    properties: DialogProperties = DialogProperties()
) {
    if (negativeButtonTitle != null && onNegativeButtonClick == null) {
        error("8e2029d-4f6a-93a6-1de4d1d00d0f")
    }
    val dismissButton: @Composable (() -> Unit)? = if (negativeButtonTitle != null) {
        {
            NegativeButton(
                text = negativeButtonTitle,
                onClick = { onNegativeButtonClick?.invoke() }
            )
        }
    } else null

    val confirmButton: @Composable (() -> Unit) = {
        PositiveButton(
            text = positiveButtonTitle,
            onClick = onPositiveButtonClick
        )
    }

    val titleComposable: @Composable (() -> Unit)? = if (title != null) {
        {
            NovaText(
                text = title,
                style = PolkadotTheme.typography.headline.small,
                color = PolkadotTheme.colors.fg.primary
            )
        }
    } else null

    val textComposable: @Composable (() -> Unit) = {
        NovaText(
            text = text,
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.primary
        )
    }

    BasicAlertDialog(
        modifier = modifier,
        onDismissRequest = onDismissRequest,
        properties = properties,
    ) {
        PolkadotSurface(
            color = PolkadotTheme.colors.bg.surface.container,
            shape = RoundedCornerShape(28.dp),
            shadowElevation = 5.dp
        ) {
            Column(
                modifier = Modifier.padding(PolkadotTheme.spacings.large),
            ) {
                titleComposable?.let {
                    it()
                    VerticalSpacer { mediumIncreased }
                }
                textComposable()
                VerticalSpacer { large }

                Row(
                    modifier = Modifier.align(Alignment.End),
                    horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)
                ) {
                    dismissButton?.invoke()
                    confirmButton()
                }
            }
        }
    }
}

@Composable
private fun PositiveButton(
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
    text: String
) {
    Button(
        modifier = modifier,
        onClick = onClick,
        text = text,
        textColor = PolkadotTheme.colors.fg.primary,
        rippleColor = Color(0x3DFFFFFF)
    )
}

@Composable
private fun NegativeButton(
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
    text: String
) {
    Button(
        modifier = modifier,
        onClick = onClick,
        text = text,
        textColor = LegacyNovaStableColors.PinkPink600,
        rippleColor = LegacyNovaStableColors.PinkPink600
    )
}

@Composable
private fun Button(
    modifier: Modifier = Modifier,
    text: String,
    textColor: Color,
    onClick: () -> Unit,
    rippleColor: Color
) {
    CompositionLocalProvider(
        LocalIndication provides ripple(color = rippleColor),
    ) {
        PolkadotSurface(
            modifier = modifier,
            shape = PolkadotTheme.shapes.small,
            color = Color.Transparent,
            onClick = onClick
        ) {
            NovaText(
                modifier = Modifier.padding(PolkadotTheme.spacings.extraMedium),
                text = text,
                color = textColor,
                textAlign = TextAlign.Center,
                style = PolkadotTheme.typography.body.medium
            )
        }
    }
}
