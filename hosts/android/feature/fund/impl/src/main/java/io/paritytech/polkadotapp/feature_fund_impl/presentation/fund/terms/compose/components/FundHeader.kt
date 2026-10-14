package io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose.components

import androidx.compose.foundation.Image
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.R
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Failure
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.KnownTokenUiConfig

@Composable
fun FundHeader(
    doneEnabled: Boolean,
    chainName: String,
    tokenUiConfig: KnownTokenUiConfig,
    doneClicked: () -> Unit,
) {
    Column {
        Box(
            modifier = Modifier.fillMaxWidth()
        ) {
            NovaText(
                modifier = Modifier
                    .padding(
                        vertical = PolkadotTheme.spacings.extraMedium,
                        horizontal = PolkadotTheme.spacings.large
                    )
                    .clickable(onClick = doneClicked)
                    .align(Alignment.CenterEnd),
                textAlign = TextAlign.End,
                text = stringResource(R.string.fund_done),
                style = PolkadotTheme.typography.title.large,
                color = if (doneEnabled) {
                    PolkadotTheme.colors.fg.primary
                } else {
                    PolkadotTheme.colors.fg.tertiary
                },
            )
        }

        VerticalSpacer { small }

        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.Center
        ) {
            NovaText(
                text = stringResource(R.string.fund_fund_from),
                style = PolkadotTheme.typography.headline.large,
                color = PolkadotTheme.colors.fg.primary,
            )

            HorizontalSpacer { small }

            Image(
                modifier = Modifier.size(32.dp),
                imageVector = tokenUiConfig.icon,
                contentDescription = "token_icon"
            )

            HorizontalSpacer { small }

            NovaText(
                text = tokenUiConfig.symbol,
                style = PolkadotTheme.typography.headline.large,
                color = tokenUiConfig.color,
            )
        }

        VerticalSpacer { extraMedium }

        NovaText(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 36.dp),
            text = stringResource(
                R.string.fund_description,
                tokenUiConfig.symbol,
                chainName
            ),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.secondary,
            textAlign = TextAlign.Center,
        )
    }
}

@Preview
@Composable
private fun FundHeaderPreview() {
    PolkadotTheme {
        FundHeader(
            doneEnabled = true,
            chainName = "Polkadot",
            tokenUiConfig = KnownTokenUiConfig(
                color = PolkadotTheme.colors.fg.warning,
                icon = NovaIcons.Failure,
                symbol = "DOT",
                name = "Polkadot"
            ),
            doneClicked = {}
        )
    }
}
