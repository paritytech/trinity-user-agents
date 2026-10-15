package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.markdown

/**
 * The text of [markdown] without its marks, one line for each block, for speech and for quotes (07-LATER §9.5). A
 * list item keeps its marker, and a link keeps its text only.
 */
internal fun markdownPlainText(markdown: String): String =
    ChatMarkdownParser.parse(markdown).joinToString(separator = "\n") { it.plainText() }.trim()

private fun MarkdownBlock.plainText(): String = when (this) {
    is MarkdownBlock.Paragraph -> inlines.plainText()
    is MarkdownBlock.Heading -> inlines.plainText()
    is MarkdownBlock.ListItem -> "$marker ${inlines.plainText()}"
    is MarkdownBlock.Quote -> blocks.joinToString(separator = "\n") { it.plainText() }
    is MarkdownBlock.Code -> text
}

private fun List<MarkdownInline>.plainText(): String = joinToString(separator = "") { inline ->
    when (inline) {
        is MarkdownInline.Text -> inline.text
        is MarkdownInline.Bold -> inline.children.plainText()
        is MarkdownInline.Italic -> inline.children.plainText()
        is MarkdownInline.Code -> inline.text
        is MarkdownInline.Link -> inline.children.plainText()
        MarkdownInline.LineBreak -> "\n"
    }
}
