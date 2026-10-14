package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.markdown

/**
 * The Markdown subset of product rich text (02-DESIGN §6.1): paragraphs, bold, italic, inline code, fenced
 * code, ordered and unordered lists, block quotes and `https` links. A heading shows as a bold paragraph.
 * Raw HTML, images, tables and other links show as their text only.
 */
internal sealed interface MarkdownBlock {
    data class Paragraph(val inlines: List<MarkdownInline>) : MarkdownBlock

    data class Heading(val inlines: List<MarkdownInline>) : MarkdownBlock

    /** [marker] is "•" for an unordered item and "N." for an ordered item. */
    data class ListItem(val marker: String, val depth: Int, val inlines: List<MarkdownInline>) : MarkdownBlock

    data class Quote(val blocks: List<MarkdownBlock>) : MarkdownBlock

    data class Code(val text: String) : MarkdownBlock
}

internal sealed interface MarkdownInline {
    data class Text(val text: String) : MarkdownInline

    data class Bold(val children: List<MarkdownInline>) : MarkdownInline

    data class Italic(val children: List<MarkdownInline>) : MarkdownInline

    data class Code(val text: String) : MarkdownInline

    data class Link(val children: List<MarkdownInline>, val url: String) : MarkdownInline

    data object LineBreak : MarkdownInline
}

internal object ChatMarkdownParser {
    private const val MAX_LIST_DEPTH = 3
    private const val SPACES_PER_DEPTH = 2

    private val fenceRegex = Regex("^ {0,3}(`{3,}|~{3,})(.*)$")
    private val headingRegex = Regex("^ {0,3}#{1,6}(?:[ \t]+(.*?))?(?:[ \t]+#+)?[ \t]*$")
    private val quoteRegex = Regex("^ {0,3}> ?(.*)$")
    private val listItemRegex = Regex("^( *)([-*+]|\\d{1,9}[.)])[ \t]+(.*)$")
    private val thematicBreakRegex = Regex("^ {0,3}([-*_])(?:[ \t]*\\1){2,}[ \t]*$")
    private val htmlTagRegex = Regex("</?[A-Za-z][A-Za-z0-9-]*(?:\\s[^<>]*)?/?>")

    fun parse(markdown: String): List<MarkdownBlock> = parseBlocks(markdown.replace("\r\n", "\n").split('\n'))

    private fun parseBlocks(lines: List<String>): List<MarkdownBlock> {
        val blocks = mutableListOf<MarkdownBlock>()
        val paragraph = mutableListOf<String>()

        fun flushParagraph() {
            if (paragraph.isEmpty()) return
            blocks += MarkdownBlock.Paragraph(parseInlines(paragraph))
            paragraph.clear()
        }

        var index = 0
        while (index < lines.size) {
            val line = lines[index]
            val fence = fenceRegex.matchEntire(line)
            val heading = headingRegex.matchEntire(line)
            val quote = quoteRegex.matchEntire(line)
            val listItem = listItemRegex.matchEntire(line)

            when {
                fence != null -> {
                    flushParagraph()
                    val closing = fence.groupValues[1]
                    val code = mutableListOf<String>()
                    index++
                    while (index < lines.size && !lines[index].trim().startsWith(closing)) {
                        code += lines[index]
                        index++
                    }
                    blocks += MarkdownBlock.Code(code.joinToString("\n"))
                }

                line.isBlank() -> flushParagraph()

                thematicBreakRegex.matches(line) -> flushParagraph()

                heading != null -> {
                    flushParagraph()
                    blocks += MarkdownBlock.Heading(parseInlines(listOf(heading.groupValues[1])))
                }

                quote != null -> {
                    flushParagraph()
                    val quoted = mutableListOf<String>()
                    while (index < lines.size) {
                        val quotedLine = quoteRegex.matchEntire(lines[index]) ?: break
                        quoted += quotedLine.groupValues[1]
                        index++
                    }
                    index--
                    blocks += MarkdownBlock.Quote(parseBlocks(quoted))
                }

                listItem != null -> {
                    flushParagraph()
                    val content = mutableListOf(listItem.groupValues[3])
                    while (index + 1 < lines.size && lines[index + 1].isListContinuation()) {
                        content += lines[index + 1].trim()
                        index++
                    }
                    blocks += MarkdownBlock.ListItem(
                        marker = listItem.groupValues[2].toMarker(),
                        depth = (listItem.groupValues[1].length / SPACES_PER_DEPTH).coerceAtMost(MAX_LIST_DEPTH),
                        inlines = parseInlines(content),
                    )
                }

                else -> paragraph += line
            }
            index++
        }

        flushParagraph()
        return blocks
    }

    private fun String.isListContinuation(): Boolean {
        return isNotBlank() && startsWith(" ") && !listItemRegex.matches(this) && !fenceRegex.matches(this)
    }

    private fun String.toMarker(): String {
        val number = dropLast(1).toIntOrNull() ?: return "•"
        return "$number."
    }

    /** The lines of one block. A line that ends with two spaces or a backslash breaks the line. */
    private fun parseInlines(lines: List<String>): List<MarkdownInline> {
        val result = mutableListOf<MarkdownInline>()

        lines.forEachIndexed { index, rawLine ->
            val isLast = index == lines.lastIndex
            val hardBreak = !isLast && (rawLine.endsWith("  ") || rawLine.endsWith("\\"))
            val line = rawLine.trim().removeSuffix("\\").let { if (hardBreak) it.trimEnd() else it }

            result += InlineParser(htmlTagRegex.replace(line, "")).parse()

            if (!isLast) {
                result += if (hardBreak) MarkdownInline.LineBreak else MarkdownInline.Text(" ")
            }
        }

        return result.mergeTexts()
    }
}

private class InlineParser(private val source: String) {
    private var position = 0

    fun parse(): List<MarkdownInline> = parseUntil(closing = null)

    private fun parseUntil(closing: String?): List<MarkdownInline> {
        val nodes = mutableListOf<MarkdownInline>()
        val text = StringBuilder()

        fun flushText() {
            if (text.isEmpty()) return
            nodes += MarkdownInline.Text(text.toString())
            text.clear()
        }

        while (position < source.length) {
            if (closing != null && isClosingAt(closing, position)) {
                flushText()
                return nodes
            }

            val char = source[position]
            val node = when (char) {
                '\\' -> escape()
                '`' -> codeSpan()
                '*', '_' -> emphasis(char)
                '!' -> image()
                '[' -> link()
                '<' -> autolink()
                else -> null
            }

            if (node != null) {
                flushText()
                nodes += node
            } else {
                text.append(source[position])
                position++
            }
        }

        flushText()
        return nodes
    }

    private fun isClosingAt(delimiter: String, at: Int): Boolean {
        if (!source.startsWith(delimiter, at)) return false
        if (at == 0 || source[at - 1].isWhitespace()) return false

        val after = source.getOrNull(at + delimiter.length)
        val intraword = delimiter.first() == '_' && after != null && after.isLetterOrDigit()
        return !intraword
    }

    private fun escape(): MarkdownInline? {
        val next = source.getOrNull(position + 1) ?: return null
        if (next !in ESCAPABLE) return null

        position += 2
        return MarkdownInline.Text(next.toString())
    }

    private fun codeSpan(): MarkdownInline? {
        val runLength = source.countFrom(position, '`')
        val delimiter = "`".repeat(runLength)
        val end = source.indexOf(delimiter, position + runLength)
        if (end < 0) return null

        val content = source.substring(position + runLength, end)
        position = end + runLength
        val trimmed = if (content.length > 2 && content.startsWith(' ') && content.endsWith(' ')) {
            content.substring(1, content.length - 1)
        } else {
            content
        }
        return MarkdownInline.Code(trimmed)
    }

    private fun emphasis(char: Char): MarkdownInline? {
        val runLength = source.countFrom(position, char).coerceAtMost(2)
        val delimiter = char.toString().repeat(runLength)
        val contentStart = position + runLength
        val next = source.getOrNull(contentStart)
        if (next == null || next.isWhitespace()) return null
        if (char == '_' && position > 0 && source[position - 1].isLetterOrDigit()) return null
        if (!hasClosing(delimiter, contentStart)) return null

        val start = position
        position = contentStart
        val children = parseUntil(delimiter)
        if (!source.startsWith(delimiter, position)) {
            position = start
            return null
        }
        position += runLength

        return if (runLength == 2) MarkdownInline.Bold(children) else MarkdownInline.Italic(children)
    }

    private fun hasClosing(delimiter: String, from: Int): Boolean {
        var at = source.indexOf(delimiter, from + 1)
        while (at >= 0) {
            if (isClosingAt(delimiter, at)) return true
            at = source.indexOf(delimiter, at + 1)
        }
        return false
    }

    /** An image shows as its alternative text. */
    private fun image(): MarkdownInline? {
        if (source.getOrNull(position + 1) != '[') return null
        val target = linkTarget(position + 1) ?: return null

        position = target.end
        return MarkdownInline.Text(target.label)
    }

    private fun link(): MarkdownInline? {
        val target = linkTarget(position) ?: return null
        position = target.end

        val children = InlineParser(target.label).parse()
        if (target.url.isAllowedLink()) return MarkdownInline.Link(children, target.url)

        return MarkdownInline.Text(children.joinToString("") { it.plainText() })
    }

    private fun autolink(): MarkdownInline? {
        val end = source.indexOf('>', position)
        if (end < 0) return null

        val url = source.substring(position + 1, end)
        if (!url.isAllowedLink() || url.any { it.isWhitespace() }) return null

        position = end + 1
        return MarkdownInline.Link(listOf(MarkdownInline.Text(url)), url)
    }

    private fun linkTarget(openBracket: Int): LinkTarget? {
        val closeBracket = source.matchingBracket(openBracket) ?: return null
        if (source.getOrNull(closeBracket + 1) != '(') return null

        val closeParen = source.indexOf(')', closeBracket + 2)
        if (closeParen < 0) return null

        val destination = source.substring(closeBracket + 2, closeParen).trim().substringBefore(' ').removeSurrounding("<", ">")
        return LinkTarget(label = source.substring(openBracket + 1, closeBracket), url = destination, end = closeParen + 1)
    }

    private fun String.matchingBracket(open: Int): Int? {
        var depth = 0
        var at = open
        while (at < length) {
            when (this[at]) {
                '\\' -> at++
                '[' -> depth++
                ']' -> {
                    depth--
                    if (depth == 0) return at
                }
            }
            at++
        }
        return null
    }

    private fun String.countFrom(start: Int, char: Char): Int {
        var count = 0
        while (start + count < length && this[start + count] == char) count++
        return count
    }

    /** Only `https` links open; any other link shows as its text. */
    private fun String.isAllowedLink(): Boolean = startsWith("https://", ignoreCase = true) && length > "https://".length

    private class LinkTarget(val label: String, val url: String, val end: Int)

    private companion object {
        const val ESCAPABLE = "\\`*_{}[]()#+-.!|<>~"
    }
}

internal fun MarkdownInline.plainText(): String = when (this) {
    is MarkdownInline.Text -> text
    is MarkdownInline.Code -> text
    is MarkdownInline.Bold -> children.joinToString("") { it.plainText() }
    is MarkdownInline.Italic -> children.joinToString("") { it.plainText() }
    is MarkdownInline.Link -> children.joinToString("") { it.plainText() }
    MarkdownInline.LineBreak -> "\n"
}

private fun List<MarkdownInline>.mergeTexts(): List<MarkdownInline> {
    val merged = mutableListOf<MarkdownInline>()
    for (node in this) {
        val last = merged.lastOrNull()
        if (node is MarkdownInline.Text && last is MarkdownInline.Text) {
            merged[merged.lastIndex] = MarkdownInline.Text(last.text + node.text)
        } else {
            merged += node
        }
    }
    return merged
}
