package io.paritytech.polkadotapp.feature_calls_impl.presentation.call.compose.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.MicOffFilled
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun MicOffWarnings(
    modifier: Modifier = Modifier,
    selfMicMuted: Boolean,
    remoteMicMuted: Boolean,
    remoteCallerName: String,
) {
    if (selfMicMuted || remoteMicMuted) {
        Column(
            modifier = modifier,
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
        ) {
            if (selfMicMuted) {
                CallStateBanner(
                    icon = NovaIcons.MicOffFilled,
                    text = stringResource(RCommon.string.call_self_mic_off),
                )
            }
            if (remoteMicMuted) {
                CallStateBanner(
                    icon = NovaIcons.MicOffFilled,
                    text = stringResource(RCommon.string.call_remote_mic_off, remoteCallerName),
                )
            }
        }
    }
}
