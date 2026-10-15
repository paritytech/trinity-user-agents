package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.text
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.avatar.PolkadotAvatar
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.BranchControlUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ReasoningRowUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ThreadChipUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.LocalChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.util.rememberReducedMotion
import io.paritytech.polkadotapp.common.R as RCommon

/** The test tags of the additions of 07-LATER §9 that the emulator launcher finds. */
internal object ProductMessageDecorationTags {
    const val THREAD_CHIP = "chat_thread_chip"
    const val THREAD_UNREAD = "chat_thread_unread"
    const val BRANCH_PREVIOUS = "chat_branch_previous"
    const val BRANCH_NEXT = "chat_branch_next"
    const val BRANCH_POSITION = "chat_branch_position"
    const val BRANCH_WAITING = "chat_branch_waiting"
    const val REASONING_ROW = "chat_reasoning_row"
    const val REASONING_TEXT = "chat_reasoning_text"
}

private val ChipAvatarSize = 16.dp
private val ChipAvatarOverlap = 6.dp
private val UnreadDotSize = 8.dp
private val BranchSpinnerSize = 12.dp
private const val THINKING_LINES = 6
private const val THINKING_PULSE_MS = 700
private const val THINKING_MIN_ALPHA = 0.35f
private const val MS_PER_SECOND = 1_000

/**
 * The chip under the root of a thread: the count of replies, up to three repliers, the time of the last reply, and a
 * dot while the thread has unread replies. A tap opens the thread room (07-LATER §9.2).
 */
@Composable
internal fun ThreadChip(
    modifier: Modifier = Modifier,
    chip: ThreadChipUiModel,
    onOpen: () -> Unit,
) {
    val replies = pluralStringResource(RCommon.plurals.chat_thread_replies, chip.replyCount.toInt(), chip.replyCount.toInt())
    val time = chip.lastReplyAt?.let { LocalChatMessageTimeFormatter.current.formatMessageTime(it) }
    val label = listOfNotNull(replies, time).joinToString(separator = " · ")
    val shape = PolkadotTheme.shapes.full

    PolkadotSurface(
        modifier = modifier
            .padding(top = PolkadotTheme.spacings.tiny)
            .clip(shape)
            .clickable(role = Role.Button, onClickLabel = stringResource(RCommon.string.chat_thread_open), onClick = onOpen)
            .testTag(ProductMessageDecorationTags.THREAD_CHIP),
        shape = shape,
        color = PolkadotTheme.colors.bg.surface.container,
        border = BorderStroke(PolkadotTheme.borders.default, PolkadotTheme.colors.stroke.secondary),
    ) {
        Row(
            modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.small, vertical = PolkadotTheme.spacings.tiny),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (chip.repliers.isNotEmpty()) {
                // The avatars of the repliers overlap a little.
                Row(horizontalArrangement = Arrangement.spacedBy(-ChipAvatarOverlap)) {
                    chip.repliers.take(MAX_REPLIERS).forEach { replier ->
                        PolkadotAvatar(model = replier.avatar, modifier = Modifier.size(ChipAvatarSize))
                    }
                }
                HorizontalSpacer { small }
            }

            NovaText(
                text = label,
                style = PolkadotTheme.typography.body.smallEmphasized,
                color = PolkadotTheme.colors.fg.secondary,
                maxLines = 1,
            )

            if (chip.hasUnread) {
                HorizontalSpacer { small }
                val newReplies = stringResource(RCommon.string.chat_thread_new_replies)
                Box(
                    modifier = Modifier
                        .size(UnreadDotSize)
                        .clip(CircleShape)
                        .background(PolkadotTheme.colors.fg.primary)
                        .semantics { contentDescription = newReplies }
                        .testTag(ProductMessageDecorationTags.THREAD_UNREAD),
                )
            }
        }
    }
}

/**
 * "‹ index/count ›" under a message with alternatives. An arrow sends `BranchSelected`; while the product does not
 * answer, the control shows a spinner and takes no arrow (07-LATER §9.3).
 */
@Composable
internal fun BranchControl(
    modifier: Modifier = Modifier,
    branch: BranchControlUiModel,
    onSelect: (index: Int) -> Unit,
) {
    val position = stringResource(RCommon.string.chat_branch_position, branch.index, branch.count)
    Row(
        modifier = modifier.padding(top = PolkadotTheme.spacings.tiny),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.tiny),
    ) {
        BranchArrow(
            symbol = "‹",
            description = stringResource(RCommon.string.chat_branch_previous),
            enabled = !branch.waiting && branch.index > 1,
            tag = ProductMessageDecorationTags.BRANCH_PREVIOUS,
            onClick = { onSelect(branch.index - 1) },
        )
        NovaText(
            modifier = Modifier
                .semantics { contentDescription = position }
                .testTag(ProductMessageDecorationTags.BRANCH_POSITION),
            text = "${branch.index}/${branch.count}",
            style = PolkadotTheme.typography.body.smallEmphasized,
            color = PolkadotTheme.colors.fg.secondary,
        )
        BranchArrow(
            symbol = "›",
            description = stringResource(RCommon.string.chat_branch_next),
            enabled = !branch.waiting && branch.index < branch.count,
            tag = ProductMessageDecorationTags.BRANCH_NEXT,
            onClick = { onSelect(branch.index + 1) },
        )
        if (branch.waiting) {
            val waiting = stringResource(RCommon.string.chat_branch_waiting)
            NovaCircularProgressIndicator(
                modifier = Modifier
                    .size(BranchSpinnerSize)
                    .semantics { contentDescription = waiting }
                    .testTag(ProductMessageDecorationTags.BRANCH_WAITING),
                strokeWidth = 1.5.dp,
            )
        }
    }
}

@Composable
private fun BranchArrow(symbol: String, description: String, enabled: Boolean, tag: String, onClick: () -> Unit) {
    val color = if (enabled) PolkadotTheme.colors.fg.primary else PolkadotTheme.colors.fg.tertiary
    Box(
        modifier = Modifier
            .clip(CircleShape)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .semantics {
                contentDescription = description
                if (!enabled) disabled()
            }
            .testTag(tag)
            .padding(horizontal = PolkadotTheme.spacings.small, vertical = PolkadotTheme.spacings.tiny),
        contentAlignment = Alignment.Center,
    ) {
        NovaText(text = symbol, style = PolkadotTheme.typography.body.large, color = color)
    }
}

/**
 * A reasoning row in place of a bubble: a dimmed, smaller text. While the product thinks, it reads "Thinking…" and
 * shows the streamed text up to six lines. Then it collapses to "Thought for N s", and a tap expands it. TalkBack
 * reads "Reasoning, collapsed" or "Reasoning, expanded" (07-LATER §9.4).
 */
@Composable
internal fun ReasoningRow(
    modifier: Modifier = Modifier,
    reasoning: ReasoningRowUiModel,
    text: String?,
) {
    var expanded by rememberSaveable { mutableStateOf(false) }
    val open = reasoning.thinking || expanded
    val title = when {
        reasoning.thinking -> stringResource(RCommon.string.chat_reasoning_thinking)
        reasoning.durationMs != null ->
            stringResource(RCommon.string.chat_reasoning_thought_for, (reasoning.durationMs + MS_PER_SECOND / 2) / MS_PER_SECOND)
        else -> stringResource(RCommon.string.chat_reasoning_thought)
    }
    val state = stringResource(if (open) RCommon.string.chat_reasoning_expanded else RCommon.string.chat_reasoning_collapsed)
    val description = "${stringResource(RCommon.string.chat_reasoning)}, $state"
    val toggle = { if (!reasoning.thinking) expanded = !expanded }

    Column(modifier = modifier.testTag(ProductMessageDecorationTags.REASONING_ROW)) {
        Row(
            modifier = Modifier
                .clickable(enabled = !reasoning.thinking, role = Role.Button, onClick = toggle)
                .clearAndSetSemantics {
                    // TalkBack reads the description, for example "Reasoning, collapsed"; the text stays for tests.
                    contentDescription = description
                    this.text = AnnotatedString(title)
                    role = Role.Button
                    if (!reasoning.thinking) {
                        onClick {
                            toggle()
                            true
                        }
                    }
                }
                .padding(vertical = PolkadotTheme.spacings.tiny),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // The pulse stops when the system animations are off.
            val pulses = reasoning.thinking && !rememberReducedMotion()
            NovaText(
                modifier = if (pulses) Modifier.alpha(thinkingAlpha()) else Modifier,
                text = title,
                style = PolkadotTheme.typography.body.smallEmphasized,
                color = PolkadotTheme.colors.fg.tertiary,
            )
            if (!reasoning.thinking) {
                HorizontalSpacer { tiny }
                NovaText(
                    text = if (expanded) "▾" else "▸",
                    style = PolkadotTheme.typography.body.small,
                    color = PolkadotTheme.colors.fg.tertiary,
                )
            }
        }
        if (open && !text.isNullOrBlank()) {
            NovaText(
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag(ProductMessageDecorationTags.REASONING_TEXT),
                text = text,
                style = PolkadotTheme.typography.body.small,
                color = PolkadotTheme.colors.fg.tertiary,
                maxLines = if (reasoning.thinking) THINKING_LINES else Int.MAX_VALUE,
                overflow = if (reasoning.thinking) TextOverflow.Ellipsis else TextOverflow.Clip,
            )
        }
    }
}

@Composable
private fun thinkingAlpha(): Float {
    val transition = rememberInfiniteTransition(label = "thinking")
    val alpha by transition.animateFloat(
        initialValue = 1f,
        targetValue = THINKING_MIN_ALPHA,
        animationSpec = infiniteRepeatable(tween(THINKING_PULSE_MS), RepeatMode.Reverse),
        label = "thinking alpha",
    )
    return alpha
}

private const val MAX_REPLIERS = 3
