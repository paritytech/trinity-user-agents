@file:OptIn(ExperimentalMaterial3Api::class)

package io.paritytech.polkadotapp.app.root.presentation.root.compose

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaModalBottomSheet
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun DevResetOverlay(
    isVisible: Boolean,
    onStartOverClick: () -> Unit,
    onDismissClick: () -> Unit,
) {
    PolkadotTheme {
        NovaModalBottomSheet(
            isVisible = isVisible,
            onDismissRequest = onDismissClick,
        ) {
            DevResetSheetContent(
                onStartOverClick = onStartOverClick,
                onDismissClick = onDismissClick,
            )
        }
    }
}

@Composable
private fun DevResetSheetContent(
    onStartOverClick: () -> Unit,
    onDismissClick: () -> Unit,
) {
    Column(
        modifier = Modifier.padding(
            horizontal = PolkadotTheme.spacings.mediumIncreased,
            vertical = PolkadotTheme.spacings.large,
        ),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        NovaText(
            text = stringResource(RCommon.string.dev_reset_title),
            style = PolkadotTheme.typography.headline.small,
            textAlign = TextAlign.Center,
        )

        VerticalSpacer { mediumIncreased }

        NovaText(
            text = stringResource(RCommon.string.dev_reset_message),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.secondary,
            textAlign = TextAlign.Center,
        )

        VerticalSpacer { extraLarge }

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.dev_reset_start_over),
            style = PolkadotButtonStyle.primary(),
            onClick = onStartOverClick,
        )

        VerticalSpacer { small }

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.dev_reset_dismiss),
            style = PolkadotButtonStyle.ghost(),
            onClick = onDismissClick,
        )
    }
}

@Preview
@Composable
private fun DevResetOverlayPreview() {
    DevResetOverlay(
        isVisible = true,
        onStartOverClick = {},
        onDismissClick = {},
    )
}
