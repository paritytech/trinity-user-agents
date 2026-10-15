package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.markdown

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.TextUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Markdown follows the font size of the user: no span of the subset sets a size of its own (P8.8, TalkBack and font scale). */
class ChatMarkdownFontScaleTest {
    private val styles = ChatMarkdownStyles(codeBackground = Color.Gray, quoteColor = Color.DarkGray)

    @Test
    fun `no span of the markdown subset fixes a font size`() {
        val markdown = """
            # Heading
            **bold** *italic* `code` [link](https://example.com)

            - item
              1. nested

            > quote

            ```
            fenced code
            ```
        """.trimIndent()

        val text = ChatMarkdownParser.parse(markdown).toAnnotatedString(styles)

        assertTrue(text.spanStyles.isNotEmpty())
        text.spanStyles.forEach { span -> assertEquals(TextUnit.Unspecified, span.item.fontSize) }
    }
}
