package io.paritytech.polkadotapp.feature_calls_impl.presentation.call.compose.components

import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.avatar.PolkadotAvatar
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_calls_impl.presentation.call.models.CallUiState
import io.paritytech.polkadotapp.feature_calls_impl.presentation.call.models.CallerDisplayUiModel
import kotlin.time.Duration
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun CallerInfo(
    state: CallUiState,
    callerDisplay: CallerDisplayUiModel,
    showAvatar: Boolean = false,
) {
    if (showAvatar && callerDisplay.avatar != null) {
        PolkadotAvatar(
            model = callerDisplay.avatar,
            modifier = Modifier.size(108.dp),
        )
        VerticalSpacer { mediumIncreased }
    }

    CompanionName(callerDisplay.name)

    VerticalSpacer { tiny }

    CallStatusText(state)
}

@Composable
fun CompanionName(name: String) {
    NovaText(
        text = name,
        style = PolkadotTheme.typography.headline.small,
        color = PolkadotTheme.colors.fg.primary,
        textAlign = TextAlign.Center,
        maxLines = 1
    )
}

@Composable
fun CallStatusText(state: CallUiState) {
    NovaText(
        text = callStatusText(state),
        style = PolkadotTheme.typography.body.large,
        color = PolkadotTheme.colors.fg.primary,
        textAlign = TextAlign.Center,
    )
}

@Composable
private fun callStatusText(state: CallUiState): String = when (state) {
    is CallUiState.Initializing -> stringResource(RCommon.string.call_initializing)
    is CallUiState.Incoming -> stringResource(RCommon.string.call_incoming)
    is CallUiState.Outgoing -> when (state.status) {
        CallUiState.Outgoing.Status.Requesting -> stringResource(RCommon.string.call_requesting)
        CallUiState.Outgoing.Status.Ringing -> stringResource(RCommon.string.call_ringing)
    }
    is CallUiState.Connecting -> stringResource(RCommon.string.call_connecting)
    is CallUiState.InProgress -> formatDuration(state.duration)
    is CallUiState.Ended -> stringResource(RCommon.string.call_ended)
    is CallUiState.Failed -> stringResource(RCommon.string.call_failed)
}

private fun formatDuration(duration: Duration): String =
    duration.toComponents { minutes, seconds, _ -> "%02d:%02d".format(minutes, seconds) }
