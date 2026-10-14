package io.paritytech.polkadotapp.feature_usernames_impl.presentation.integrityFailed.compose

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import io.paritytech.polkadotapp.design.components.spacer.FillerSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.integrityFailed.IntegrityFailedViewModel
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun IntegrityFailedScreen(viewModel: IntegrityFailedViewModel) {
    BackHandler { viewModel.backPressed() }

    IntegrityFailedScreenInternal()
}

@Composable
private fun IntegrityFailedScreenInternal() {
    PolkadotSurface {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .background(PolkadotTheme.colors.bg.surface.main)
                .systemBarsPadding()
                .padding(horizontal = PolkadotTheme.spacings.medium),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            FillerSpacer()

            NovaText(
                modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.extraLarge),
                text = stringResource(RCommon.string.integrity_failed_title),
                style = PolkadotTheme.typography.headline.large,
                color = PolkadotTheme.colors.fg.primary,
                textAlign = TextAlign.Center
            )

            VerticalSpacer { extraMedium }

            NovaText(
                modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.extraLarge),
                text = stringResource(RCommon.string.integrity_failed_description),
                style = PolkadotTheme.typography.body.large,
                color = PolkadotTheme.colors.fg.tertiary,
                textAlign = TextAlign.Center
            )

            FillerSpacer()
        }
    }
}

@Preview
@Composable
private fun IntegrityFailedScreenPreview() {
    PolkadotTheme {
        IntegrityFailedScreenInternal()
    }
}
