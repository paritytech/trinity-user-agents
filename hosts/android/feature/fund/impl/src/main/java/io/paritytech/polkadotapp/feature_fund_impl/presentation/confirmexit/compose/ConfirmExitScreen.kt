package io.paritytech.polkadotapp.feature_fund_impl.presentation.confirmexit.compose

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.R
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_fund_impl.presentation.confirmexit.ConfirmExitContract

@Composable
fun ConfirmExitScreen(contract: ConfirmExitContract) {
    ConfirmExitScreenInternal(
        confirmClicked = contract::confirmClicked,
        dismissClicked = contract::dismissClicked
    )
}

@Composable
private fun ConfirmExitScreenInternal(
    confirmClicked: () -> Unit,
    dismissClicked: () -> Unit,
) {
    PolkadotSurface(
        modifier = Modifier
            .systemBarsPadding()
            .padding(PolkadotTheme.spacings.small),
        color = PolkadotTheme.colors.bg.surface.nested,
        shape = PolkadotTheme.shapes.extraLarge
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(PolkadotTheme.spacings.mediumIncreased),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            VerticalSpacer { mediumIncreased }

            NovaText(
                modifier = Modifier.padding(horizontal = 32.dp),
                textAlign = TextAlign.Center,
                text = stringResource(id = R.string.fund_exit_confirm_title),
                style = PolkadotTheme.typography.headline.small,
                color = PolkadotTheme.colors.fg.primary,
            )

            VerticalSpacer { small }

            NovaText(
                modifier = Modifier.padding(horizontal = 32.dp),
                textAlign = TextAlign.Center,
                text = stringResource(id = R.string.fund_exit_confirm_description),
                style = PolkadotTheme.typography.body.large,
                color = PolkadotTheme.colors.fg.tertiary,
            )

            VerticalSpacer { extraLarge }

            Row {
                PolkadotTextButton(
                    modifier = Modifier.weight(1f),
                    text = stringResource(R.string.fund_exit_confirm_CTA_dismiss),
                    style = PolkadotButtonStyle.secondary(),
                    onClick = dismissClicked
                )

                HorizontalSpacer { small }

                PolkadotTextButton(
                    modifier = Modifier.weight(1f),
                    text = stringResource(R.string.fund_exit_confirm_CTA_confirm),
                    style = PolkadotButtonStyle.primary(),
                    onClick = confirmClicked
                )
            }
        }
    }
}

@Preview
@Composable
private fun ConfirmExitScreenInternalPreview() {
    PolkadotTheme {
        ConfirmExitScreenInternal(
            confirmClicked = {},
            dismissClicked = {}
        )
    }
}
