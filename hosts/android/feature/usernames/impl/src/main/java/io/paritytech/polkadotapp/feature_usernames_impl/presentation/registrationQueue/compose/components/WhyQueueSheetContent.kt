package io.paritytech.polkadotapp.feature_usernames_impl.presentation.registrationQueue.compose.components

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
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotButton
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun WhyQueueSheetContent(onGotItClicked: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                horizontal = PolkadotTheme.spacings.medium,
                vertical = PolkadotTheme.spacings.medium
            )
    ) {
        NovaText(
            text = stringResource(RCommon.string.registration_queue_why_title),
            style = PolkadotTheme.typography.headline.medium,
            color = PolkadotTheme.colors.fg.primary
        )

        VerticalSpacer { extraMedium }

        NovaText(
            text = stringResource(RCommon.string.registration_queue_why_description),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.tertiary
        )

        VerticalSpacer { medium }

        PolkadotSurface(
            modifier = Modifier.fillMaxWidth(),
            shape = PolkadotTheme.shapes.large,
            color = PolkadotTheme.colors.bg.surface.container
        ) {
            Column(modifier = Modifier.padding(PolkadotTheme.spacings.medium)) {
                WhyQueuePoint(number = 1, text = stringResource(RCommon.string.registration_queue_why_point_secure))
                VerticalSpacer { medium }
                WhyQueuePoint(number = 2, text = stringResource(RCommon.string.registration_queue_why_point_fair))
                VerticalSpacer { medium }
                WhyQueuePoint(number = 3, text = stringResource(RCommon.string.registration_queue_why_point_abuse))
            }
        }

        VerticalSpacer { extraLarge }

        PolkadotButton(
            modifier = Modifier.fillMaxWidth(),
            onClick = onGotItClicked,
            style = PolkadotButtonStyle.secondary()
        ) {
            NovaText(text = stringResource(RCommon.string.common_got_it))
        }
    }
}

@Composable
private fun WhyQueuePoint(number: Int, text: String) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        PolkadotSurface(
            shape = PolkadotTheme.shapes.full,
            color = PolkadotTheme.colors.bg.surface.nested
        ) {
            Box(
                modifier = Modifier.size(32.dp),
                contentAlignment = Alignment.Center
            ) {
                NovaText(
                    text = number.toString(),
                    style = PolkadotTheme.typography.body.medium,
                    color = PolkadotTheme.colors.fg.primary
                )
            }
        }

        HorizontalSpacer { extraMedium }

        NovaText(
            text = text,
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.primary
        )
    }
}

@Preview
@Composable
private fun WhyQueueSheetContentPreview() {
    PolkadotTheme {
        WhyQueueSheetContent(onGotItClicked = {})
    }
}
