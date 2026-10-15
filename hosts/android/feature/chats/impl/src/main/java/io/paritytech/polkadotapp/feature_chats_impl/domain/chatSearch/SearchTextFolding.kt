package io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch

import java.text.Normalizer
import java.util.Locale

// The text matching of the in-room search: case and diacritics do not count (04-TESTING S61).

/**
 * A text without case and diacritics. [sourceIndex] gives, for each char of [text], the index of the char of the
 * source text that it comes from, so a match in [text] can mark the same chars in the source text.
 */
internal class FoldedText(val text: String, private val sourceIndex: IntArray, private val sourceLength: Int) {
    /** The chars of the source text that the chars [range] of [text] come from. */
    fun sourceRange(range: IntRange): IntRange {
        val first = sourceIndex[range.first]
        val afterLast = if (range.last + 1 < sourceIndex.size) sourceIndex[range.last + 1] else sourceLength
        return first until maxOf(afterLast, first + 1)
    }
}

/** Removes the case and the diacritics of [source]: "Crème" becomes "creme". */
internal fun foldForSearch(source: String): FoldedText {
    if (source.isAscii()) {
        return FoldedText(source.lowercase(Locale.ROOT), IntArray(source.length) { it }, source.length)
    }

    val folded = StringBuilder(source.length)
    val sourceIndex = ArrayList<Int>(source.length)
    var index = 0
    while (index < source.length) {
        val codePoint = source.codePointAt(index)
        val width = Character.charCount(codePoint)
        val piece = if (codePoint < ASCII_END) {
            codePoint.toChar().lowercaseChar().toString()
        } else {
            foldCodePoint(source.substring(index, index + width))
        }
        piece.forEach { char ->
            folded.append(char)
            sourceIndex.add(index)
        }
        index += width
    }

    return FoldedText(folded.toString(), sourceIndex.toIntArray(), source.length)
}

/** The query as the search compares it: without case, diacritics and the spaces at its ends. */
internal fun foldQuery(query: String): String = foldForSearch(query.trim()).text

/** The ranges of [source] where [foldedQuery] occurs, case and diacritics ignored. Empty when it does not occur. */
internal fun searchMatches(source: String, foldedQuery: String): List<IntRange> {
    if (foldedQuery.isEmpty()) return emptyList()

    val folded = foldForSearch(source)
    val ranges = mutableListOf<IntRange>()
    var from = folded.text.indexOf(foldedQuery)
    while (from >= 0) {
        ranges += folded.sourceRange(from until from + foldedQuery.length)
        from = folded.text.indexOf(foldedQuery, startIndex = from + foldedQuery.length)
    }

    return ranges
}

private fun foldCodePoint(text: String): String = Normalizer.normalize(text, Normalizer.Form.NFD)
    .filter { Character.getType(it) != Character.NON_SPACING_MARK.toInt() }
    .lowercase(Locale.ROOT)

private fun String.isAscii(): Boolean = all { it.code < ASCII_END }

private const val ASCII_END = 0x80
