package io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SearchTextFoldingTest {
    @Test
    fun `case and diacritics do not count`() {
        assertEquals("creme brulee", foldQuery("  Crème BRÛLÉE "))
        assertEquals(listOf(4..8), searchMatches("Une crème brûlée", foldQuery("CREME")))
        assertEquals(listOf(4..8), searchMatches("Une creme brulee", foldQuery("crème")))
    }

    @Test
    fun `a match marks the chars of the source, also when a diacritic is a char of its own`() {
        val decomposed = "café ok"

        val matches = searchMatches(decomposed, foldQuery("café"))

        assertEquals(listOf(0..4), matches)
        assertEquals("café", decomposed.substring(matches.single()))
    }

    @Test
    fun `each occurrence is a match`() {
        assertEquals(listOf(0..2, 8..10), searchMatches("Ölé and ole", foldQuery("ole")))
    }

    @Test
    fun `a text outside the Latin alphabet matches without case`() {
        assertEquals(listOf(0..5), searchMatches("Привет мир", foldQuery("привет")))
        assertEquals(listOf(3..4), searchMatches("ab 日本 cd", foldQuery("日本")))
    }

    @Test
    fun `a text without the query, or an empty query, has no match`() {
        assertTrue(searchMatches("Plain text message", foldQuery("crème")).isEmpty())
        assertTrue(searchMatches("Plain text message", foldQuery("   ")).isEmpty())
    }
}
