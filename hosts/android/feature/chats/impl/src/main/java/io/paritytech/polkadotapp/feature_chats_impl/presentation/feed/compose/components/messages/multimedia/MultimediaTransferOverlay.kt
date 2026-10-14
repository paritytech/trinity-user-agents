package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.multimedia

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.animateIntAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.AlertOutlined
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowDownward
import io.paritytech.polkadotapp.design.components.icon.vectors.Close
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.presentation.transfer.FileTransferDirection
import io.paritytech.polkadotapp.feature_chats_api.presentation.transfer.FileTransferUiState
import io.paritytech.polkadotapp.common.R as RCommon

private const val TRANSFER_PROGRESS_ANIMATION_MILLIS = 400
private const val TRANSFER_PROGRESS_SPIN_MILLIS = 1000
private const val MIN_VISIBLE_TRANSFER_PROGRESS = 0.01f

@Composable
internal fun TransferStatePill(
    modifier: Modifier,
    state: FileTransferUiState
) {
    when (state) {
        is FileTransferUiState.InProgress -> {
            val percent by animateIntAsState(
                targetValue = state.progress.inPercents.toInt(),
                animationSpec = tween(durationMillis = TRANSFER_PROGRESS_ANIMATION_MILLIS),
                label = "TransferProgressPercent"
            )
            val text = when (state.direction) {
                FileTransferDirection.UPLOAD ->
                    stringResource(RCommon.string.chat_message_multimedia_uploading_progress, percent)

                FileTransferDirection.DOWNLOAD ->
                    stringResource(RCommon.string.chat_message_multimedia_downloading_progress, percent)
            }
            PillSurface(
                modifier = modifier,
                backgroundColor = PolkadotTheme.colors.bg.surface.overlay,
                showAlert = false,
                text = text
            )
        }

        is FileTransferUiState.Failed -> {
            val text = when (state.direction) {
                FileTransferDirection.UPLOAD ->
                    stringResource(RCommon.string.chat_message_multimedia_uploading_failed)

                FileTransferDirection.DOWNLOAD ->
                    stringResource(RCommon.string.chat_message_multimedia_downloading_failed)
            }
            PillSurface(
                modifier = modifier,
                backgroundColor = PolkadotTheme.colors.bg.status.error,
                showAlert = true,
                text = text
            )
        }

        is FileTransferUiState.Cancelled -> Unit
    }
}

@Composable
private fun PillSurface(
    modifier: Modifier,
    backgroundColor: Color,
    showAlert: Boolean,
    text: String
) {
    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.full,
        color = backgroundColor
    ) {
        Row(
            modifier = Modifier
                .padding(
                    horizontal = PolkadotTheme.spacings.tiny,
                    vertical = PolkadotTheme.spacings.extraTiny
                ),
            verticalAlignment = Alignment.CenterVertically
        ) {
            if (showAlert) {
                NovaIcon(
                    modifier = Modifier.size(12.dp),
                    imageVector = NovaIcons.AlertOutlined,
                    tint = PolkadotTheme.colors.fg.primary
                )
            }

            NovaText(
                modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.tiny),
                text = text,
                style = PolkadotTheme.typography.body.small,
                color = PolkadotTheme.colors.fg.primary
            )
        }
    }
}

@Composable
internal fun TransferStateControl(
    modifier: Modifier,
    state: FileTransferUiState?,
    onRedownload: () -> Unit,
    onCancel: () -> Unit
) {
    when (state) {
        is FileTransferUiState.Failed -> Unit

        is FileTransferUiState.Cancelled -> {
            TransferControlButton(
                modifier = modifier,
                icon = NovaIcons.ArrowDownward,
                onClick = onRedownload
            )
        }

        is FileTransferUiState.InProgress -> {
            val fraction by animateFloatAsState(
                targetValue = state.progress.fraction.toFloat().coerceAtLeast(MIN_VISIBLE_TRANSFER_PROGRESS),
                animationSpec = tween(durationMillis = TRANSFER_PROGRESS_ANIMATION_MILLIS),
                label = "TransferProgressRing"
            )
            val spin by rememberInfiniteTransition(label = "TransferProgressSpin").animateFloat(
                initialValue = 0f,
                targetValue = 360f,
                animationSpec = infiniteRepeatable(
                    animation = tween(durationMillis = TRANSFER_PROGRESS_SPIN_MILLIS, easing = LinearEasing),
                    repeatMode = RepeatMode.Restart
                ),
                label = "TransferProgressSpinAngle"
            )

            Box(
                modifier = modifier
                    .size(MULTIMEDIA_CONTROL_DIAMETER)
                    .clip(CircleShape)
                    .background(PolkadotTheme.colors.bg.surface.overlay),
                contentAlignment = Alignment.Center
            ) {
                NovaCircularProgressIndicator(
                    modifier = Modifier
                        .fillMaxSize()
                        .rotate(spin),
                    progress = { fraction },
                    color = PolkadotTheme.colors.fg.primary
                )

                NovaIcon(
                    modifier = Modifier
                        .size(MULTIMEDIA_CONTROL_ICON_SIZE)
                        .clickable(onClick = onCancel),
                    imageVector = NovaIcons.Close,
                    tint = PolkadotTheme.colors.fg.primary
                )
            }
        }

        null -> Unit
    }
}

@Composable
private fun TransferControlButton(
    modifier: Modifier,
    icon: ImageVector,
    onClick: () -> Unit
) {
    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.full,
        color = PolkadotTheme.colors.bg.surface.overlay,
        onClick = onClick
    ) {
        Box(
            modifier = Modifier.size(MULTIMEDIA_CONTROL_DIAMETER),
            contentAlignment = Alignment.Center
        ) {
            NovaIcon(
                modifier = Modifier.size(MULTIMEDIA_CONTROL_ICON_SIZE),
                imageVector = icon,
                tint = PolkadotTheme.colors.fg.primary
            )
        }
    }
}
