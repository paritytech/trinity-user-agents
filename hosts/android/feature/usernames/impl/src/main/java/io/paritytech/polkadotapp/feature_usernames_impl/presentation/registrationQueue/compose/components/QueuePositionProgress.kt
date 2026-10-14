package io.paritytech.polkadotapp.feature_usernames_impl.presentation.registrationQueue.compose.components

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

private const val QUEUE_PROGRESS_SPIN_MILLIS = 2000
private const val MIN_VISIBLE_QUEUE_PROGRESS = 0.01f

@Composable
internal fun QueuePositionProgress(position: Int, progress: Float) {
    val spin by rememberInfiniteTransition(label = "RegistrationQueueProgressSpin")
        .animateFloat(
            initialValue = 0f,
            targetValue = 360f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = QUEUE_PROGRESS_SPIN_MILLIS, easing = LinearEasing),
                repeatMode = RepeatMode.Restart
            ),
            label = "RegistrationQueueProgressSpinAngle"
        )

    Box(contentAlignment = Alignment.Center) {
        NovaCircularProgressIndicator(
            modifier = Modifier
                .matchParentSize()
                .aspectRatio(1f)
                .rotate(spin),
            progress = { progress.coerceAtLeast(MIN_VISIBLE_QUEUE_PROGRESS) },
            color = PolkadotTheme.colors.fg.success,
            trackColor = PolkadotTheme.colors.bg.surface.container,
            strokeWidth = 16.dp,
            strokeCap = StrokeCap.Round
        )

        Column(
            modifier = Modifier.padding(72.dp),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            NovaText(
                text = position.toString(),
                style = PolkadotTheme.typography.display.medium,
                color = PolkadotTheme.colors.fg.primary
            )
            NovaText(
                text = stringResource(RCommon.string.registration_queue_position_caption),
                style = PolkadotTheme.typography.body.medium,
                color = PolkadotTheme.colors.fg.primary
            )
        }
    }
}
