package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages

import android.net.Uri
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.image.NovaAsyncImage
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ReplyPreview
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.paymentSubtitle
import io.paritytech.polkadotapp.common.R as RCommon

private val ReplyThumbnailSize = 48.dp

@Composable
fun ReplyPreviewBubble(
    modifier: Modifier,
    preview: ReplyPreview,
    direction: ChatMessageUiModel.Direction,
    onClick: () -> Unit
) {
    val backgroundColor = when (direction) {
        ChatMessageUiModel.Direction.INCOMING -> PolkadotTheme.colors.bg.surface.nested
        ChatMessageUiModel.Direction.OUTGOING -> PolkadotTheme.colors.bg.surface.nestedInverted
    }

    val textColor = when (direction) {
        ChatMessageUiModel.Direction.INCOMING -> PolkadotTheme.colors.fg.primary
        ChatMessageUiModel.Direction.OUTGOING -> PolkadotTheme.colors.fg.primaryInverted
    }

    PolkadotSurface(
        modifier = modifier,
        color = backgroundColor,
        shape = PolkadotTheme.shapes.medium,
        onClick = onClick
    ) {
        Row(
            modifier = Modifier.height(IntrinsicSize.Min)
        ) {
            ReplyAccentBar()

            when (val content = preview.content) {
                is ReplyPreview.Content.Image -> ReplyThumbnail(content.thumbnailUri, direction)
                is ReplyPreview.Content.Video -> ReplyThumbnail(content.thumbnailUri, direction)

                is ReplyPreview.Content.Text,
                is ReplyPreview.Content.File,
                is ReplyPreview.Content.Payment -> Unit
            }

            Column(
                modifier = Modifier.padding(PolkadotTheme.spacings.small)
            ) {
                NovaText(
                    text = preview.title,
                    style = PolkadotTheme.typography.title.tiny,
                    color = textColor,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )

                NovaText(
                    text = preview.content.subtitle(PolkadotTheme.typography.body.smallEmphasized),
                    style = PolkadotTheme.typography.body.smallEmphasized,
                    color = textColor,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis
                )
            }
        }
    }
}

@Composable
private fun ReplyThumbnail(uri: Uri?, direction: ChatMessageUiModel.Direction) {
    val borderColor = when (direction) {
        ChatMessageUiModel.Direction.INCOMING -> PolkadotTheme.colors.stroke.secondary
        ChatMessageUiModel.Direction.OUTGOING -> PolkadotTheme.colors.stroke.primaryInverted
    }

    PolkadotSurface(
        modifier = Modifier
            .padding(
                start = PolkadotTheme.spacings.small,
                top = PolkadotTheme.spacings.small,
                bottom = PolkadotTheme.spacings.small
            )
            .size(ReplyThumbnailSize),
        shape = PolkadotTheme.shapes.small,
        color = PolkadotTheme.colors.bg.surface.container,
        border = BorderStroke(PolkadotTheme.borders.default, borderColor)
    ) {
        NovaAsyncImage(
            modifier = Modifier.fillMaxSize(),
            model = uri,
            contentScale = ContentScale.Crop
        )
    }
}

@Composable
private fun ReplyAccentBar() {
    Box(
        modifier = Modifier
            .width(PolkadotTheme.borders.large)
            .fillMaxHeight()
            .background(PolkadotTheme.colors.stroke.tertiary)
    )
}

@Composable
private fun ReplyPreview.Content.subtitle(style: TextStyle): AnnotatedString {
    return when (this) {
        is ReplyPreview.Content.Text -> AnnotatedString(text)
        is ReplyPreview.Content.Image -> AnnotatedString(caption ?: stringResource(RCommon.string.chat_attachment_name_image))
        is ReplyPreview.Content.Video -> AnnotatedString(caption ?: stringResource(RCommon.string.chat_attachment_name_video))
        is ReplyPreview.Content.File -> AnnotatedString(caption ?: fileName)
        is ReplyPreview.Content.Payment -> paymentSubtitle(style)
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ReplyPreviewBubblePreview() {
    PolkadotTheme {
        Column(
            modifier = Modifier
                .width(260.dp)
                .padding(PolkadotTheme.spacings.medium),
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)
        ) {
            val variants = listOf(
                ReplyPreview.Content.Text("short message short message short message short message short"),
                ReplyPreview.Content.Image(thumbnailUri = null, caption = "short message short message"),
                ReplyPreview.Content.Image(thumbnailUri = null, caption = null),
                ReplyPreview.Content.Video(thumbnailUri = null, caption = "check out this clip"),
                ReplyPreview.Content.Video(thumbnailUri = null, caption = null),
                ReplyPreview.Content.File(fileName = "report-q3.pdf", caption = null)
            )

            variants.forEach { content ->
                ReplyPreviewBubble(
                    modifier = Modifier,
                    preview = ReplyPreview(messageId = "1", title = "Jake.23", content = content),
                    direction = ChatMessageUiModel.Direction.INCOMING,
                    onClick = {}
                )
            }
        }
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF1A1A1A)
@Composable
private fun ReplyPreviewBubbleOutgoingPreview() {
    PolkadotTheme {
        Column(
            modifier = Modifier
                .width(260.dp)
                .padding(PolkadotTheme.spacings.medium),
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)
        ) {
            val variants = listOf(
                ReplyPreview.Content.Text("short message short message short"),
                ReplyPreview.Content.Image(thumbnailUri = null, caption = null),
                ReplyPreview.Content.File(fileName = "report-q3.pdf", caption = null)
            )

            variants.forEach { content ->
                ReplyPreviewBubble(
                    modifier = Modifier,
                    preview = ReplyPreview(messageId = "1", title = "Jake.23", content = content),
                    direction = ChatMessageUiModel.Direction.OUTGOING,
                    onClick = {}
                )
            }
        }
    }
}
