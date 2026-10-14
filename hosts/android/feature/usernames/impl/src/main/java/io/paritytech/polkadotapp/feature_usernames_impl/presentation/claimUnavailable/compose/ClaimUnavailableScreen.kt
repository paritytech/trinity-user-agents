package io.paritytech.polkadotapp.feature_usernames_impl.presentation.claimUnavailable.compose

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.spacer.FillerSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_usernames_impl.R
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.claimUnavailable.ClaimUnavailableViewModel
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun ClaimUnavailableScreen(viewModel: ClaimUnavailableViewModel) {
    BackHandler { viewModel.backPressed() }

    ClaimUnavailableScreenInternal(
        onRecoverClicked = viewModel::onRecoverClicked
    )
}

@Composable
private fun ClaimUnavailableScreenInternal(onRecoverClicked: () -> Unit) {
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

            Image(
                modifier = Modifier.padding(
                    horizontal = 72.dp,
                    vertical = 24.dp
                ),
                painter = painterResource(R.drawable.img_verification_not_available),
                contentDescription = "img"
            )

            NovaText(
                modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.extraLarge),
                text = stringResource(RCommon.string.claim_unavailable_title),
                style = PolkadotTheme.typography.title.large,
                color = PolkadotTheme.colors.fg.primary,
                textAlign = TextAlign.Center
            )

            VerticalSpacer { extraMedium }

            NovaText(
                modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.extraLarge),
                text = stringResource(RCommon.string.claim_unavailable_description),
                style = PolkadotTheme.typography.body.large,
                color = PolkadotTheme.colors.fg.tertiary,
                textAlign = TextAlign.Center
            )

            VerticalSpacer { extraLarge }

            PolkadotSurface(onClick = onRecoverClicked) {
                NovaText(
                    text = buildRecoverHereText(),
                    style = PolkadotTheme.typography.body.medium
                )
            }

            FillerSpacer()
        }
    }
}

@Composable
private fun buildRecoverHereText(): AnnotatedString {
    val recoverHereText = stringResource(RCommon.string.claim_username_recover_here)
    val fullText = stringResource(RCommon.string.claim_username_already_have_account, recoverHereText)

    return buildAnnotatedString {
        withStyle(SpanStyle(color = PolkadotTheme.colors.fg.tertiary)) {
            append(fullText)
        }
        addStyle(
            SpanStyle(color = PolkadotTheme.colors.fg.primary),
            fullText.indexOf(recoverHereText),
            fullText.length
        )
    }
}

@Preview
@Composable
private fun ClaimUnavailableScreenPreview() {
    PolkadotTheme {
        ClaimUnavailableScreenInternal(onRecoverClicked = {})
    }
}
