package io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.models

import androidx.compose.runtime.Immutable
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toImmutableList

private const val ELLIPSIS = "…"

private const val LEADING_CONTEXT_THRESHOLD = 24
private const val LEADING_CONTEXT_LENGTH = 12

@Immutable
data class SearchExcerpt(
    val text: String,
    val highlights: ImmutableList<IntRange>,
)

internal fun buildSearchExcerpt(source: String, query: String): SearchExcerpt {
    val collapsed = source.collapseWhitespace()
    if (query.isEmpty()) return SearchExcerpt(collapsed, persistentListOf())

    val matches = collapsed.allMatchRanges(query)
    val firstMatchStart = matches.firstOrNull()?.first ?: return SearchExcerpt(collapsed, persistentListOf())

    if (firstMatchStart <= LEADING_CONTEXT_THRESHOLD) {
        return SearchExcerpt(collapsed, matches.toImmutableList())
    }

    val cutAt = collapsed.wordStartBefore(firstMatchStart - LEADING_CONTEXT_LENGTH)
    val windowed = ELLIPSIS + collapsed.substring(cutAt)
    val shift = ELLIPSIS.length - cutAt

    return SearchExcerpt(
        text = windowed,
        highlights = matches.map { it.first + shift..it.last + shift }.toImmutableList(),
    )
}

private fun String.collapseWhitespace(): String = trim().replace(WHITESPACE_RUN, " ")

private fun String.allMatchRanges(query: String): List<IntRange> {
    val ranges = mutableListOf<IntRange>()
    var from = indexOf(query, startIndex = 0, ignoreCase = true)

    while (from >= 0) {
        ranges += from until from + query.length
        from = indexOf(query, startIndex = from + query.length, ignoreCase = true)
    }

    return ranges
}

private fun String.wordStartBefore(index: Int): Int {
    val safeIndex = index.coerceIn(0, length)
    val lastSpace = lastIndexOf(' ', safeIndex)

    return if (lastSpace < 0) safeIndex else lastSpace + 1
}

private val WHITESPACE_RUN = Regex("\\s+")
