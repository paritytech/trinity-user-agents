package io.paritytech.polkadotapp.common.utils

import android.icu.lang.UCharacter
import android.icu.lang.UProperty
import android.net.Uri
import androidx.core.net.toUri
import java.math.RoundingMode
import java.text.BreakIterator
import java.util.Locale

/**
 * Replaces all parts in form of '{name}' to the corresponding value from values using 'name' as a key.
 *
 * @return formatted string
 */
fun String.formatNamed(values: Map<String, String>): String {
    return formatNamed(values, onUnknownSecret = { "null" })
}

fun String.formatNamedOrThrow(values: Map<String, String>): String {
    return formatNamed(
        values,
        onUnknownSecret = { throw IllegalArgumentException("Unknown secret: $it") })
}

fun String.formatNamed(vararg values: Pair<String, String>) = formatNamed(values.toMap())

fun String.formatNamed(
    values: Map<String, String>,
    onUnknownSecret: (secretName: String) -> String,
): String {
    return NAMED_PATTERN_REGEX.replace(this) { matchResult ->
        val argumentName = matchResult.groupValues.second()

        values[argumentName] ?: onUnknownSecret(argumentName)
    }
}

fun String.removeHexPrefix() = removePrefix("0x")

fun String.requireSuffix(suffix: String): String = if (endsWith(suffix)) this else this + suffix

fun String?.nullIfEmpty(): String? = if (isNullOrEmpty()) null else this

fun String.filterDigits(precision: Int, tempSymbol: String = "#") = this
    .replace(",", ".")
    .replaceFirst(".", tempSymbol)
    .replace(".", "")
    .replace(tempSymbol, ".")
    .filter { it.isDigit() || it == '.' }
    .let { if (it == ".") "" else it }
    .limitDecimalPlaces(precision)

fun String.limitDecimalPlaces(precision: Int): String {
    if (lastOrNull() == '.') return this
    val bd = toBigDecimalOrNull() ?: return this
    return if (bd.scale() > precision) {
        bd.setScale(precision, RoundingMode.DOWN).toPlainString()
    } else {
        bd.toPlainString()
    }
}

private val NAMED_PATTERN_REGEX = "\\{([a-zA-z]+)\\}".toRegex()

fun String.toUriResult(): Result<Uri> = runCatching { toUri() }

fun String.capitalize(locale: Locale = Locale.getDefault()): String {
    return this.replaceFirstChar {
        if (it.isLowerCase()) it.titlecase(locale) else it.toString()
    }
}

fun String.isSingleEmoji(): Boolean {
    val trimmed = this.trim()
    if (trimmed.isEmpty()) return false

    val it = BreakIterator.getCharacterInstance()
    it.setText(trimmed)

    var count = 0
    while (it.next() != BreakIterator.DONE) {
        count++
    }

    if (count == 1) {
        val firstCodePoint = trimmed.codePointAt(0)
        return UCharacter.hasBinaryProperty(firstCodePoint, UProperty.EMOJI) &&
            !Character.isLetterOrDigit(firstCodePoint) &&
            firstCodePoint > 0xFF
    }

    return false
}
