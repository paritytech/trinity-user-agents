package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.markdown

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.markdown.MarkdownBlock.Code
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.markdown.MarkdownBlock.Heading
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.markdown.MarkdownBlock.ListItem
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.markdown.MarkdownBlock.Paragraph
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.markdown.MarkdownBlock.Quote
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.markdown.MarkdownInline.Bold
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.markdown.MarkdownInline.Italic
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.markdown.MarkdownInline.Link
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.markdown.MarkdownInline.Text
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.markdown.MarkdownInline.Code as InlineCode

class ChatMarkdownParserTest {
    private val styles = ChatMarkdownStyles(codeBackground = Color.Gray, quoteColor = Color.LightGray)

    @Test
    fun `plain text is one paragraph`() {
        assertEquals(listOf(Paragraph(listOf(Text("Hello there")))), parse("Hello there"))
    }

    @Test
    fun `a blank line separates paragraphs and a single line break is a space`() {
        assertEquals(
            listOf(Paragraph(listOf(Text("one two"))), Paragraph(listOf(Text("three")))),
            parse("one\ntwo\n\nthree"),
        )
    }

    @Test
    fun `two trailing spaces break the line`() {
        assertEquals(listOf(Paragraph(listOf(Text("one"), MarkdownInline.LineBreak, Text("two")))), parse("one  \ntwo"))
    }

    @Test
    fun `bold, italic and inline code`() {
        assertEquals(
            listOf(
                Paragraph(
                    listOf(
                        Bold(listOf(Text("bold"))),
                        Text(" and "),
                        Italic(listOf(Text("italic"))),
                        Text(" and "),
                        Italic(listOf(Text("also"))),
                        Text(" and "),
                        InlineCode("x = 1"),
                    )
                )
            ),
            parse("**bold** and *italic* and _also_ and `x = 1`"),
        )
    }

    @Test
    fun `nested emphasis`() {
        assertEquals(
            listOf(Paragraph(listOf(Italic(listOf(Text("a "), Bold(listOf(Text("b"))), Text(" c")))))),
            parse("*a **b** c*"),
        )
    }

    @Test
    fun `an underscore inside a word and a lone star stay text`() {
        assertEquals(listOf(Paragraph(listOf(Text("snake_case_name costs 2 * 3")))), parse("snake_case_name costs 2 * 3"))
    }

    @Test
    fun `a backslash escapes a marker`() {
        assertEquals(listOf(Paragraph(listOf(Text("*not italic*")))), parse("\\*not italic\\*"))
    }

    @Test
    fun `an https link keeps its destination`() {
        assertEquals(
            listOf(Paragraph(listOf(Text("See "), Link(listOf(Text("docs")), "https://example.com/a"), Text(".")))),
            parse("See [docs](https://example.com/a)."),
        )
    }

    @Test
    fun `a link that is not https shows as its text`() {
        assertEquals(listOf(Paragraph(listOf(Text("Open app now")))), parse("Open [app](polkadot://x) now"))
        assertEquals(listOf(Paragraph(listOf(Text("plain http")))), parse("plain [http](http://example.com)"))
    }

    @Test
    fun `an angle bracket https link is a link`() {
        assertEquals(
            listOf(Paragraph(listOf(Link(listOf(Text("https://example.com")), "https://example.com")))),
            parse("<https://example.com>"),
        )
    }

    @Test
    fun `an image shows as its alternative text`() {
        assertEquals(listOf(Paragraph(listOf(Text("a cat")))), parse("![a cat](https://example.com/cat.png)"))
    }

    @Test
    fun `raw html shows as its text`() {
        assertEquals(listOf(Paragraph(listOf(Text("bold text")))), parse("<b>bold</b> text"))
    }

    @Test
    fun `a heading is a bold paragraph`() {
        assertEquals(listOf(Heading(listOf(Text("Title")))), parse("## Title"))
        assertEquals(listOf(Paragraph(listOf(Text("#hashtag")))), parse("#hashtag"))
    }

    @Test
    fun `fenced code keeps its lines as they are`() {
        assertEquals(listOf(Code("val a = 1\n  **b**")), parse("```kotlin\nval a = 1\n  **b**\n```"))
    }

    @Test
    fun `lists keep their markers and depth`() {
        assertEquals(
            listOf(
                ListItem("•", 0, listOf(Text("one"))),
                ListItem("•", 1, listOf(Text("nested"))),
                ListItem("2.", 0, listOf(Bold(listOf(Text("two"))))),
            ),
            parse("- one\n  - nested\n2. **two**"),
        )
    }

    @Test
    fun `a block quote holds blocks`() {
        assertEquals(listOf(Quote(listOf(Paragraph(listOf(Text("quoted text")))))), parse("> quoted\n> text"))
    }

    @Test
    fun `a thematic break shows nothing`() {
        assertEquals(listOf(Paragraph(listOf(Text("a"))), Paragraph(listOf(Text("b")))), parse("a\n\n---\n\nb"))
    }

    @Test
    fun `the text keeps the block structure as characters`() {
        val text = ChatMarkdownParser.parse("# Menu\n\n- tea\n- coffee\n\n> hot").toAnnotatedString(styles)

        assertEquals("Menu\n\n• tea\n• coffee\n\n▎ hot", text.text)
    }

    @Test
    fun `the text styles bold and italic runs and links the url`() {
        val text = ChatMarkdownParser.parse("**a** *b* [c](https://example.com)").toAnnotatedString(styles)

        assertEquals("a b c", text.text)
        assertTrue(text.spanStyles.any { it.item.fontWeight == FontWeight.Bold && it.start == 0 && it.end == 1 })
        assertTrue(text.spanStyles.any { it.item.fontStyle == FontStyle.Italic && it.start == 2 && it.end == 3 })

        val link = text.getLinkAnnotations(4, 5).single()
        assertEquals("https://example.com", (link.item as LinkAnnotation.Url).url)
    }

    private fun parse(markdown: String) = ChatMarkdownParser.parse(markdown)
}
