package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.call

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import io.paritytech.polkadotapp.common.utils.rememberCurrentTimeMillisWithDelay
import io.paritytech.polkadotapp.design.components.avatar.PolkadotAvatar
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonShape
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotButtonSize
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButtonSize
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.CallEndFilled
import io.paritytech.polkadotapp.design.components.icon.vectors.CallFilled
import io.paritytech.polkadotapp.design.components.icon.vectors.VideocamFilled
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import kotlin.time.Duration.Companion.seconds
import io.paritytech.polkadotapp.common.R as RCommon

/** The test tags of the call parts of a product room, which the E2E launcher reads. */
object ProductRoomCallTags {
    const val AUDIO_BUTTON = "chat_call_button_audio"
    const val VIDEO_BUTTON = "chat_call_button_video"
    const val RINGING_BANNER = "chat_call_ringing_banner"
    const val ACCEPT = "chat_call_accept"
    const val DECLINE = "chat_call_decline"
    const val BAR = "chat_call_bar"
    const val TIMER = "chat_call_timer"
    const val END = "chat_call_end"
    const val SCREEN = "chat_call_screen"
    const val IN_APP_BANNER = "chat_call_in_app_banner"
}

/** The call buttons of the header of a product room. A press asks the product for a call (`CallRequested`). */
@Composable
internal fun ProductRoomCallButtons(buttons: ProductRoomCallUiState.Buttons, onRequest: (video: Boolean) -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        if (buttons.audio) {
            CallHeaderButton(
                tag = ProductRoomCallTags.AUDIO_BUTTON,
                label = stringResource(RCommon.string.product_room_call_audio),
                video = false,
                onRequest = onRequest,
            )
        }
        if (buttons.video) {
            CallHeaderButton(
                tag = ProductRoomCallTags.VIDEO_BUTTON,
                label = stringResource(RCommon.string.product_room_call_video),
                video = true,
                onRequest = onRequest,
            )
        }
    }
}

@Composable
private fun CallHeaderButton(tag: String, label: String, video: Boolean, onRequest: (video: Boolean) -> Unit) {
    PolkadotIconButton(
        modifier = Modifier
            .testTag(tag)
            .semantics { contentDescription = label },
        icon = if (video) NovaIcons.VideocamFilled else NovaIcons.CallFilled,
        onClick = { onRequest(video) },
        style = PolkadotButtonStyle.ghost(),
        size = PolkadotIconButtonSize.mediumIncreased(),
        shape = PolkadotButtonShape.pill,
    )
}

/** The ringing banner or the call bar under the header of a product room. Nothing without a call. */
@Composable
internal fun ProductRoomCallSlot(call: ProductRoomCallUiState.Call?, onAction: (ProductRoomCallAction) -> Unit) {
    when (call) {
        is ProductRoomCallUiState.Call.Ringing -> RingingBanner(call, onAction)
        is ProductRoomCallUiState.Call.Active -> CallBar(call, onAction)
        null -> Unit
    }
}

@Composable
private fun RingingBanner(call: ProductRoomCallUiState.Call.Ringing, onAction: (ProductRoomCallAction) -> Unit) {
    Row(
        modifier = Modifier
            .testTag(ProductRoomCallTags.RINGING_BANNER)
            .fillMaxWidth()
            .background(PolkadotTheme.colors.bg.surface.container)
            .padding(horizontal = PolkadotTheme.spacings.mediumIncreased, vertical = PolkadotTheme.spacings.small),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.medium),
    ) {
        call.callerAvatar?.let { PolkadotAvatar(model = it, modifier = Modifier.size(CallerAvatarSize)) }
        Column(modifier = Modifier.weight(1f)) {
            NovaText(
                text = stringResource(ringingTitleRes(call.video)),
                style = PolkadotTheme.typography.body.mediumEmphasized,
                color = PolkadotTheme.colors.fg.primary,
                maxLines = 1,
            )
            call.callerName?.let {
                NovaText(
                    text = it,
                    style = PolkadotTheme.typography.paragraph.medium,
                    color = PolkadotTheme.colors.fg.secondary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        PolkadotTextButton(
            modifier = Modifier.testTag(ProductRoomCallTags.DECLINE),
            text = stringResource(RCommon.string.product_room_call_decline),
            style = PolkadotButtonStyle.destructive(),
            size = PolkadotButtonSize.medium(),
            onClick = { onAction(ProductRoomCallAction.Answer(accept = false)) },
        )
        PolkadotTextButton(
            modifier = Modifier.testTag(ProductRoomCallTags.ACCEPT),
            text = stringResource(RCommon.string.product_room_call_accept),
            style = PolkadotButtonStyle.primary(),
            size = PolkadotButtonSize.medium(),
            onClick = { onAction(ProductRoomCallAction.Answer(accept = true)) },
        )
    }
}

@Composable
private fun CallBar(call: ProductRoomCallUiState.Call.Active, onAction: (ProductRoomCallAction) -> Unit) {
    val now by rememberCurrentTimeMillisWithDelay(1.seconds)
    Row(
        modifier = Modifier
            .testTag(ProductRoomCallTags.BAR)
            .fillMaxWidth()
            .background(PolkadotTheme.colors.bg.surface.container)
            .clickable(role = Role.Button) { onAction(ProductRoomCallAction.Open) }
            .padding(horizontal = PolkadotTheme.spacings.mediumIncreased, vertical = PolkadotTheme.spacings.small),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.medium),
    ) {
        NovaIcon(
            modifier = Modifier.size(CallIconSize),
            imageVector = if (call.video) NovaIcons.VideocamFilled else NovaIcons.CallFilled,
            tint = PolkadotTheme.colors.fg.success,
        )
        NovaText(
            modifier = Modifier.weight(1f),
            text = stringResource(if (call.video) RCommon.string.product_room_call_video else RCommon.string.product_room_call_bar),
            style = PolkadotTheme.typography.body.mediumEmphasized,
            color = PolkadotTheme.colors.fg.primary,
            maxLines = 1,
        )
        NovaText(
            modifier = Modifier.testTag(ProductRoomCallTags.TIMER),
            text = callElapsedText(call.startedAt, now),
            style = PolkadotTheme.typography.paragraph.medium,
            color = PolkadotTheme.colors.fg.secondary,
        )
        PolkadotTextButton(
            modifier = Modifier.testTag(ProductRoomCallTags.END),
            text = stringResource(RCommon.string.product_room_call_end),
            style = PolkadotButtonStyle.destructive(),
            size = PolkadotButtonSize.medium(),
            iconStart = NovaIcons.CallEndFilled,
            onClick = { onAction(ProductRoomCallAction.End) },
        )
    }
}

/**
 * The call screen of the host for a call that a session of the Media service carries. It is a shell until the Media
 * service is on this branch (O29): the room, the timer and End, and no pictures or device controls.
 */
@Composable
internal fun ProductRoomCallScreen(
    call: ProductRoomCallUiState.Call.Active,
    roomName: String,
    onAction: (ProductRoomCallAction) -> Unit,
) {
    val now by rememberCurrentTimeMillisWithDelay(1.seconds)
    Dialog(
        onDismissRequest = { onAction(ProductRoomCallAction.CloseScreen) },
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        Column(
            modifier = Modifier
                .testTag(ProductRoomCallTags.SCREEN)
                .fillMaxSize()
                .background(PolkadotTheme.colors.bg.surface.main)
                .padding(PolkadotTheme.spacings.large),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            NovaText(text = roomName, style = PolkadotTheme.typography.title.medium, color = PolkadotTheme.colors.fg.primary)
            NovaText(
                text = callElapsedText(call.startedAt, now),
                style = PolkadotTheme.typography.paragraph.medium,
                color = PolkadotTheme.colors.fg.secondary,
            )
            Spacer(modifier = Modifier.height(PolkadotTheme.spacings.large))
            NovaText(
                text = stringResource(RCommon.string.product_room_call_media_unavailable),
                style = PolkadotTheme.typography.paragraph.medium,
                color = PolkadotTheme.colors.fg.tertiary,
                textAlign = TextAlign.Center,
            )
            Spacer(modifier = Modifier.height(PolkadotTheme.spacings.large))
            PolkadotTextButton(
                text = stringResource(RCommon.string.product_room_call_end),
                style = PolkadotButtonStyle.destructive(),
                iconStart = NovaIcons.CallEndFilled,
                onClick = { onAction(ProductRoomCallAction.End) },
            )
        }
    }
}

private val CallerAvatarSize = 36.dp
private val CallIconSize = 20.dp
