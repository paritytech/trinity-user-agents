@file:OptIn(ExperimentalMaterial3Api::class)

package io.paritytech.polkadotapp.design.components.dialog

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.BasicAlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.DialogProperties
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotButtonSize
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

private val DialogShadowElevation = 5.dp

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

    BasicAlertDialog(
        modifier = modifier,
        onDismissRequest = onDismissRequest,
        properties = properties,
    ) {
        AlertDialogContent(
            text = text,
            title = title,
            positiveButtonTitle = positiveButtonTitle,
            onPositiveButtonClick = onPositiveButtonClick,
            negativeButtonTitle = negativeButtonTitle,
            onNegativeButtonClick = { onNegativeButtonClick?.invoke() },
        )
    }
}

@Composable
private fun AlertDialogContent(
    text: String,
    title: String?,
    positiveButtonTitle: String,
    onPositiveButtonClick: () -> Unit,
    negativeButtonTitle: String?,
    onNegativeButtonClick: () -> Unit,
) {
    PolkadotSurface(
        color = PolkadotTheme.colors.bg.surface.container,
        shape = PolkadotTheme.shapes.extraLarge,
        shadowElevation = DialogShadowElevation
    ) {
        Column(
            modifier = Modifier.padding(PolkadotTheme.spacings.large),
        ) {
            if (title != null) {
                NovaText(
                    text = title,
                    style = PolkadotTheme.typography.headline.small,
                    color = PolkadotTheme.colors.fg.primary
                )
                VerticalSpacer { mediumIncreased }
            }
            NovaText(
                text = text,
                style = PolkadotTheme.typography.body.large,
                color = PolkadotTheme.colors.fg.primary
            )
            VerticalSpacer { large }

            FlowRow(
                modifier = Modifier.align(Alignment.End),
                horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small, Alignment.End),
                verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)
            ) {
                if (negativeButtonTitle != null) {
                    PolkadotTextButton(
                        text = negativeButtonTitle,
                        style = PolkadotButtonStyle.ghost(),
                        size = PolkadotButtonSize.medium(),
                        onClick = onNegativeButtonClick
                    )
                }
                PolkadotTextButton(
                    text = positiveButtonTitle,
                    style = PolkadotButtonStyle.primary(),
                    size = PolkadotButtonSize.medium(),
                    onClick = onPositiveButtonClick
                )
            }
        }
    }
}

@Preview
@Composable
private fun AlertDialogContentPreview() {
    PolkadotTheme {
        AlertDialogContent(
            text = "Requested permissions are required to use this screen. You should enable them in Settings.",
            title = "Permissions denied",
            positiveButtonTitle = "Open Settings",
            onPositiveButtonClick = {},
            negativeButtonTitle = "Cancel",
            onNegativeButtonClick = {},
        )
    }
}
