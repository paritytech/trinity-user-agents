package io.paritytech.polkadotapp.feature_transactions.api.presentation.outcome.compose

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_transactions.api.presentation.outcome.TransactionOutcomeContract

@Composable
fun TransactionOutcomeScreen(
    contract: TransactionOutcomeContract,
    config: TransactionOutcomeUiConfig,
    details: @Composable ColumnScope.() -> Unit = {},
) {
    BackHandler { contract.onBackClick() }
    TransactionOutcomeScreenInternal(
        config = config,
        onButtonClick = contract::onButtonClick,
        details = details,
    )
}

@Composable
fun TransactionOutcomeScreenInternal(
    config: TransactionOutcomeUiConfig,
    onButtonClick: () -> Unit,
    details: @Composable ColumnScope.() -> Unit = {},
) {
    PolkadotSurface {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .systemBarsPadding()
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .align(Alignment.Center)
                    .padding(horizontal = PolkadotTheme.spacings.mediumIncreased),
                horizontalAlignment = Alignment.CenterHorizontally
            ) {
                Image(
                    modifier = Modifier
                        .padding(20.dp)
                        .size(80.dp, 80.dp),
                    imageVector = config.icon,
                    contentDescription = "outcome_image"
                )

                VerticalSpacer { 20.dp }

                NovaText(
                    text = stringResource(config.titleRes),
                    style = PolkadotTheme.typography.headline.small,
                    textAlign = TextAlign.Center
                )

                val messageRes = config.messageRes
                if (messageRes != null) {
                    VerticalSpacer { tiny }

                    NovaText(
                        text = stringResource(messageRes),
                        style = PolkadotTheme.typography.body.medium,
                        textAlign = TextAlign.Center,
                        color = PolkadotTheme.colors.fg.secondary
                    )
                }

                details()
            }

            PolkadotTextButton(
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .fillMaxWidth()
                    .padding(horizontal = PolkadotTheme.spacings.extraLarge)
                    .padding(bottom = PolkadotTheme.spacings.extraLarge),
                onClick = onButtonClick,
                text = stringResource(config.buttonRes),
            )
        }
    }
}

@Preview
@Composable
fun TransactionOutcomeScreenPreview() {
    PolkadotTheme {
        TransactionOutcomeScreenInternal(
            config = TransactionOutcomeUiConfig.defaultSuccess(),
            onButtonClick = {}
        )
    }
}
