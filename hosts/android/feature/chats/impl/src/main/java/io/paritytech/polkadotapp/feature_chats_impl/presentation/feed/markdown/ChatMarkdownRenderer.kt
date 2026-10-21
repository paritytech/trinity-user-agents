package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.markdown

import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.withStyle
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

private const val BLOCK_SEPARATOR = "\n\n"
private const val LINE_SEPARATOR = "\n"
private const val QUOTE_PREFIX = "▎ "
private const val LIST_INDENT = "    "

@Immutable
internal data class ChatMarkdownStyles(
    val codeBackground: Color,
    val quoteColor: Color,
)

@Composable
internal fun rememberMarkdownText(markdown: String): AnnotatedString {
    val styles = ChatMarkdownStyles(
        codeBackground = PolkadotTheme.colors.bg.surface.container,
        quoteColor = PolkadotTheme.colors.fg.secondary,
    )

    return remember(markdown, styles) { ChatMarkdownParser.parse(markdown).toAnnotatedString(styles) }
}

/** Draws the block structure, which a single text cannot hold, as plain characters and inline styles. */
internal fun List<MarkdownBlock>.toAnnotatedString(styles: ChatMarkdownStyles): AnnotatedString = buildAnnotatedString {
    var previous: MarkdownBlock? = null

    for (block in this@toAnnotatedString) {
        if (previous != null) {
            val staysInList = previous is MarkdownBlock.ListItem && block is MarkdownBlock.ListItem
            append(if (staysInList) LINE_SEPARATOR else BLOCK_SEPARATOR)
        }
        appendBlock(block, styles)
        previous = block
    }
}

private fun AnnotatedString.Builder.appendBlock(block: MarkdownBlock, styles: ChatMarkdownStyles) {
    when (block) {
        is MarkdownBlock.Paragraph -> appendInlines(block.inlines, styles)

        is MarkdownBlock.Heading -> withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { appendInlines(block.inlines, styles) }

        is MarkdownBlock.ListItem -> {
            append(LIST_INDENT.repeat(block.depth))
            append(block.marker)
            append(" ")
            appendInlines(block.inlines, styles)
        }

        is MarkdownBlock.Code -> withStyle(SpanStyle(fontFamily = FontFamily.Monospace, background = styles.codeBackground)) {
            append(block.text)
        }

        is MarkdownBlock.Quote -> withStyle(SpanStyle(color = styles.quoteColor)) {
            val inner = block.blocks.toAnnotatedString(styles)
            var lineStart = 0
            while (lineStart <= inner.length) {
                val lineEnd = inner.indexOf('\n', lineStart).let { if (it < 0) inner.length else it }
                append(QUOTE_PREFIX)
                append(inner.subSequence(lineStart, lineEnd))
                if (lineEnd < inner.length) append(LINE_SEPARATOR)
                lineStart = lineEnd + 1
            }
        }
    }
}

private fun AnnotatedString.Builder.appendInlines(inlines: List<MarkdownInline>, styles: ChatMarkdownStyles) {
    for (inline in inlines) {
        when (inline) {
            is MarkdownInline.Text -> append(inline.text)

            is MarkdownInline.Bold -> withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { appendInlines(inline.children, styles) }

            is MarkdownInline.Italic -> withStyle(SpanStyle(fontStyle = FontStyle.Italic)) { appendInlines(inline.children, styles) }

            is MarkdownInline.Code -> withStyle(SpanStyle(fontFamily = FontFamily.Monospace, background = styles.codeBackground)) {
                append(inline.text)
            }

            is MarkdownInline.Link -> withLink(
                LinkAnnotation.Url(
                    url = inline.url,
                    styles = TextLinkStyles(style = SpanStyle(textDecoration = TextDecoration.Underline)),
                )
            ) {
                appendInlines(inline.children, styles)
            }

            MarkdownInline.LineBreak -> append(LINE_SEPARATOR)
        }
    }
}
