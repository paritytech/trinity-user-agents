package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.markdown

import org.junit.Assert.assertEquals
import org.junit.Test

class MarkdownPlainTextTest {
    @Test
    fun `the marks go and the text stays, one line for each block`() {
        val markdown = "# Title\n\n**Bold** and *italic* with `code` and [a link](https://example.com)\n\n- one\n- two"

        assertEquals("Title\nBold and italic with code and a link\n• one\n• two", markdownPlainText(markdown))
    }

    @Test
    fun `fenced code keeps its text without the fence`() {
        assertEquals("let x = 1", markdownPlainText("```\nlet x = 1\n```"))
    }
}
