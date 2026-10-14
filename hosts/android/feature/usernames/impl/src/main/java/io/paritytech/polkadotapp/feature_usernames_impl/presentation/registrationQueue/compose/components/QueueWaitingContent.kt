package io.paritytech.polkadotapp.feature_usernames_impl.presentation.registrationQueue.compose.components

import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.registrationQueue.RegistrationQueueState
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun QueueWaitingContent(state: RegistrationQueueState) {
    QueuePositionProgress(position = state.position, progress = state.progress)

    VerticalSpacer { extraLarge }

    NovaText(
        text = stringResource(RCommon.string.registration_queue_title),
        style = PolkadotTheme.typography.title.extraLarge,
        color = PolkadotTheme.colors.fg.primary,
        textAlign = TextAlign.Center
    )

    VerticalSpacer { extraMedium }

    NovaText(
        modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.extraLarge),
        text = stringResource(RCommon.string.registration_queue_description),
        style = PolkadotTheme.typography.body.large,
        color = PolkadotTheme.colors.fg.tertiary,
        textAlign = TextAlign.Center
    )
}
