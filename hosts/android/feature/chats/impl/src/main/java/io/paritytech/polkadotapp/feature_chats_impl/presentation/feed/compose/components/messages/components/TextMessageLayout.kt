package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components

import android.util.Patterns
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.layoutId
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.defaultTextColor
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.ChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.LocalChatMessageTimeFormatter
import kotlinx.collections.immutable.persistentListOf
import kotlin.math.ceil
import kotlin.math.max

private const val LayoutIdText = "text"
private const val LayoutIdStatus = "status"
private const val LayoutIdTimestamp = "timestamp"

/**
 * Custom layout for text messages that positions text content with an optional suffix area
 * (status and/or timestamp) at the bottom-end of the message bubble.
 *
 * Spacing (all from [PolkadotTheme.spacings]):
 * - Text has `small` padding on all four sides relative to the layout edges.
 * - The suffix is anchored to the bottom-end of the layout with `tiny` bottom padding and
 *   `extraSmall` end padding.
 * - When the suffix fits on the same line as the last line of text, the horizontal gap
 *   between text and suffix is `medium`. Otherwise the suffix moves to its own line below.
 *
 * @param status Optional composable for the message status indicator (e.g. "edited").
 * @param timestamp Optional composable for the message timestamp and delivery status.
 */
@Composable
fun TextMessageLayout(
    modifier: Modifier = Modifier,
    text: String,
    style: TextStyle,
    color: Color = Color.Unspecified,
    direction: ChatMessageUiModel.Direction,
    status: (@Composable () -> Unit)? = null,
    timestamp: (@Composable () -> Unit)? = null,
    visibleCharCount: Int = Int.MAX_VALUE,
) {
    val textPadding = PolkadotTheme.spacings.extraMedium
    val suffixBottomPadding = PolkadotTheme.spacings.small
    val suffixEndPadding = PolkadotTheme.spacings.small
    val sameLineGap = PolkadotTheme.spacings.extraMedium
    val suffixItemSpacing = PolkadotTheme.spacings.tiny
    val textToSuffixGap = 6.dp

    val textMeasurer = rememberTextMeasurer()
    val annotatedText = rememberTextWithLinks(text, direction)
    val displayedText = if (visibleCharCount >= annotatedText.length) {
        annotatedText
    } else {
        val safeCount = visibleCharCount.coerceIn(0, annotatedText.length)
        buildAnnotatedString {
            append(annotatedText.subSequence(0, safeCount))
            withStyle(SpanStyle(color = Color.Transparent)) {
                append(annotatedText.subSequence(safeCount, annotatedText.length))
            }
        }
    }

    Layout(
        modifier = modifier,
        content = {
            Box(Modifier.layoutId(LayoutIdText)) {
                NovaText(
                    text = displayedText,
                    style = style,
                    color = color
                )
            }

            if (status != null) {
                Box(Modifier.layoutId(LayoutIdStatus)) {
                    status()
                }
            }

            if (timestamp != null) {
                Box(Modifier.layoutId(LayoutIdTimestamp)) {
                    timestamp()
                }
            }
        }
    ) { measurables, constraints ->
        val textPaddingPx = textPadding.roundToPx()
        val suffixBottomPaddingPx = suffixBottomPadding.roundToPx()
        val suffixEndPaddingPx = suffixEndPadding.roundToPx()
        val sameLineGapPx = sameLineGap.roundToPx()
        val betweenSuffixItemsPx = suffixItemSpacing.roundToPx()
        val textToSuffixGapPx = textToSuffixGap.roundToPx()

        val textMaxWidth = if (constraints.hasBoundedWidth) {
            (constraints.maxWidth - 2 * textPaddingPx).coerceAtLeast(0)
        } else {
            Constraints.Infinity
        }
        val textConstraints = constraints.copy(minWidth = 0, maxWidth = textMaxWidth)

        val textMeasurable = measurables.first { it.layoutId == LayoutIdText }
        val textLayoutResult = textMeasurer.measure(
            text = annotatedText,
            style = style,
            constraints = textConstraints
        )

        var maxTextLineWidth = 0f
        for (i in 0 until textLayoutResult.lineCount) {
            val lineWidth = textLayoutResult.getLineRight(i)
            if (lineWidth > maxTextLineWidth) maxTextLineWidth = lineWidth
        }
        val textRealWidth = ceil(maxTextLineWidth).toInt()

        val textPlaceable = textMeasurable.measure(
            Constraints.fixed(textRealWidth, textLayoutResult.size.height)
        )

        val statusPlaceable = measurables.firstOrNull { it.layoutId == LayoutIdStatus }
            ?.measure(constraints.copy(minWidth = 0, maxWidth = Constraints.Infinity))
        val timestampPlaceable = measurables.firstOrNull { it.layoutId == LayoutIdTimestamp }
            ?.measure(constraints.copy(minWidth = 0, maxWidth = Constraints.Infinity))

        val betweenItems = if (statusPlaceable != null && timestampPlaceable != null) betweenSuffixItemsPx else 0
        val suffixWidth = (statusPlaceable?.width ?: 0) + betweenItems + (timestampPlaceable?.width ?: 0)
        val suffixHeight = max(statusPlaceable?.height ?: 0, timestampPlaceable?.height ?: 0)

        if (suffixWidth == 0) {
            val layoutWidth = max(textRealWidth + 2 * textPaddingPx, constraints.minWidth)
            val layoutHeight = textPlaceable.height + 2 * textPaddingPx

            return@Layout layout(layoutWidth, layoutHeight) {
                textPlaceable.place(textPaddingPx, textPaddingPx)
            }
        }

        val lastLineRight = ceil(textLayoutResult.getLineRight(textLayoutResult.lineCount - 1)).toInt()
        val sameLineTotalWidth = textPaddingPx + lastLineRight + sameLineGapPx + suffixWidth + suffixEndPaddingPx
        val fitsInLastLine = sameLineTotalWidth <= constraints.maxWidth

        val textOnlyWidth = textRealWidth + 2 * textPaddingPx
        val suffixOnlyWidth = suffixWidth + textPaddingPx + suffixEndPaddingPx

        val layoutWidth: Int
        val layoutHeight: Int
        if (fitsInLastLine) {
            layoutWidth = max(max(textOnlyWidth, sameLineTotalWidth), constraints.minWidth)
            layoutHeight = textPlaceable.height + 2 * textPaddingPx
        } else {
            layoutWidth = max(max(textOnlyWidth, suffixOnlyWidth), constraints.minWidth)
            layoutHeight = textPaddingPx + textPlaceable.height + textToSuffixGapPx + suffixHeight + suffixBottomPaddingPx
        }

        val suffixX = layoutWidth - suffixEndPaddingPx - suffixWidth
        val suffixY = layoutHeight - suffixBottomPaddingPx - suffixHeight

        layout(layoutWidth, layoutHeight) {
            textPlaceable.place(textPaddingPx, textPaddingPx)
            statusPlaceable?.place(suffixX, suffixY)
            timestampPlaceable?.place(
                suffixX + (statusPlaceable?.width ?: 0) + betweenItems,
                suffixY
            )
        }
    }
}

@Composable
private fun rememberTextWithLinks(
    text: String,
    direction: ChatMessageUiModel.Direction
): AnnotatedString {
    return remember(text, direction) {
        buildAnnotatedString {
            append(text)

            val matcher = Patterns.WEB_URL.matcher(text)

            while (matcher.find()) {
                val start = matcher.start()
                val end = matcher.end()
                val url = matcher.group()
                val normalizedUrl = normalize(url)

                addLink(
                    url = LinkAnnotation.Url(
                        url = normalizedUrl,
                        styles = TextLinkStyles(
                            style = SpanStyle(
                                textDecoration = TextDecoration.Underline
                            )
                        )
                    ),
                    start = start,
                    end = end
                )
            }
        }
    }
}

private fun normalize(url: String): String = if (!url.startsWith("http://") && !url.startsWith("https://")) {
    "https://$url"
} else {
    url
}

@Preview
@Composable
private fun TextMessageLayoutPreview() {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalChatMessageTimeFormatter provides ChatMessageTimeFormatter.mocked()
        ) {
            val message = previewTextMessage()

            Column(
                modifier = Modifier.padding(PolkadotTheme.spacings.extraMedium),
                verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraMedium)
            ) {
                LayoutCase(width = 160.dp) {
                    TextMessageLayout(
                        text = "Hi",
                        style = PolkadotTheme.typography.body.large,
                        color = message.direction.defaultTextColor,
                        direction = message.direction,
                        timestamp = { FlatMessageTimestamp(message = message) }
                    )
                }

                LayoutCase(width = 160.dp) {
                    TextMessageLayout(
                        text = "Single line that almost fills the row",
                        style = PolkadotTheme.typography.body.large,
                        color = message.direction.defaultTextColor,
                        direction = message.direction,
                        timestamp = { FlatMessageTimestamp(message = message) }
                    )
                }

                LayoutCase(width = 220.dp) {
                    TextMessageLayout(
                        text = "Two lines that share the bubble width with a short tail at the end",
                        style = PolkadotTheme.typography.body.large,
                        color = message.direction.defaultTextColor,
                        direction = message.direction,
                        timestamp = { FlatMessageTimestamp(message = message) }
                    )
                }

                LayoutCase(width = 220.dp) {
                    TextMessageLayout(
                        text = "Three lines whose last line is wide enough that the timestamp has to wrap underneath it",
                        style = PolkadotTheme.typography.body.large,
                        color = message.direction.defaultTextColor,
                        direction = message.direction,
                        timestamp = { FlatMessageTimestamp(message = message) }
                    )
                }

                LayoutCase(width = 280.dp) {
                    TextMessageLayout(
                        text = "Edited message with status + timestamp on the last line",
                        style = PolkadotTheme.typography.body.large,
                        color = message.direction.defaultTextColor,
                        direction = message.direction,
                        status = { FlatEditedLabel(direction = message.direction) },
                        timestamp = { FlatMessageTimestamp(message = message) }
                    )
                }

                LayoutCase(width = 180.dp) {
                    TextMessageLayout(
                        text = "Edited message whose suffix doesn't fit on the last line so it wraps under",
                        style = PolkadotTheme.typography.body.large,
                        color = message.direction.defaultTextColor,
                        direction = message.direction,
                        status = { FlatEditedLabel(direction = message.direction) },
                        timestamp = { FlatMessageTimestamp(message = message) }
                    )
                }

                LayoutCase(width = 200.dp) {
                    TextMessageLayout(
                        text = "Plain text, no suffix",
                        style = PolkadotTheme.typography.body.large,
                        color = message.direction.defaultTextColor,
                        direction = message.direction
                    )
                }
            }
        }
    }
}

@Composable
private fun LayoutCase(
    width: Dp,
    content: @Composable () -> Unit
) {
    Box(
        modifier = Modifier
            .background(Color.White)
            .widthIn(max = width)
    ) {
        content()
    }
}

private fun previewTextMessage(
    direction: ChatMessageUiModel.Direction = ChatMessageUiModel.Direction.OUTGOING
): ChatMessageUiModel.Text {
    return ChatMessageUiModel.Text(
        id = "preview",
        timestamp = 0L,
        direction = direction,
        status = ChatMessageUiModel.Status.READ,
        origin = ChatMessageOrigin.User,
        text = "",
        replyPreview = null,
        reactions = persistentListOf(),
        isEdited = false
    )
}
