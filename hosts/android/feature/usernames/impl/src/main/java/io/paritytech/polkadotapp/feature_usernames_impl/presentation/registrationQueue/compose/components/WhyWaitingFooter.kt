package io.paritytech.polkadotapp.feature_usernames_impl.presentation.registrationQueue.compose.components

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowRight
import io.paritytech.polkadotapp.design.components.icon.vectors.HelpOutlined
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun WhyWaitingFooter(onWhyClicked: () -> Unit) {
    PolkadotSurface(onClick = onWhyClicked) {
        Row(
            modifier = Modifier.padding(PolkadotTheme.spacings.extraSmall),
            verticalAlignment = Alignment.CenterVertically
        ) {
            NovaIcon(
                imageVector = NovaIcons.HelpOutlined,
                contentDescription = null,
                tint = PolkadotTheme.colors.fg.tertiary
            )

            HorizontalSpacer { extraSmall }

            NovaText(
                text = stringResource(RCommon.string.registration_queue_why_waiting),
                style = PolkadotTheme.typography.body.medium,
                color = PolkadotTheme.colors.fg.secondary
            )

            HorizontalSpacer { extraSmall }

            NovaIcon(
                imageVector = NovaIcons.ArrowRight,
                contentDescription = null,
                tint = PolkadotTheme.colors.fg.tertiary
            )
        }
    }
}
