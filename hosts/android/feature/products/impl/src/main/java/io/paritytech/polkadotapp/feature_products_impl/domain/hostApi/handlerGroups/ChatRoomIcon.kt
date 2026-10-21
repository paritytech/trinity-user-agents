package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups

import java.net.URI

/** The core's `CHAT_ICON_MAX_BYTES`. */
private const val CHAT_ICON_MAX_BYTES = 64 * 1024

/** The core's `ALLOWED_ICON_DATA_TYPES`. SVG is excluded: it can carry script. */
private val ALLOWED_ICON_DATA_TYPES = setOf("image/png", "image/jpeg", "image/gif", "image/webp", "image/avif")

/**
 * The core's `validate_chat_icon`, for the rooms a product registers through the container bridge, which
 * the core never sees. An icon is absent, an `https` URL, or an inline PNG, JPEG, GIF, WebP or AVIF image,
 * within the core's size limit. Null when absent; anything else fails the call, as it does in the core.
 */
internal fun validateChatIcon(icon: String?): Result<String?> {
    val trimmed = icon?.trim().orEmpty()
    if (trimmed.isEmpty()) return Result.success(null)
    if (trimmed.toByteArray(Charsets.UTF_8).size > CHAT_ICON_MAX_BYTES) {
        return chatIconError("icon must be at most $CHAT_ICON_MAX_BYTES bytes")
    }
    if (trimmed.codePoints().anyMatch(::isDisplayUnsafe)) {
        return chatIconError("icon must not contain control or bidirectional characters")
    }

    val scheme = iconScheme(trimmed)
    return when {
        scheme == "https" -> validateHttpsIcon(trimmed)
        scheme == "data" && isAllowedIconDataUrl(trimmed) -> Result.success(trimmed)
        else -> chatIconError("icon carries a scheme that cannot be rendered")
    }
}

private fun validateHttpsIcon(icon: String): Result<String?> {
    val uri = runCatching { URI(icon) }.getOrNull()
        ?.takeIf { it.scheme.equals("https", ignoreCase = true) }
        ?: return chatIconError("icon carries a scheme that cannot be rendered")
    if (uri.userInfo != null) return chatIconError("icon must not carry credentials")

    // Measured as resolved, as the core does: percent-encoding grows a non-ASCII path threefold.
    val resolved = uri.toASCIIString()
    if (resolved.length > CHAT_ICON_MAX_BYTES) return chatIconError("icon must be at most $CHAT_ICON_MAX_BYTES bytes")

    return Result.success(resolved)
}

/** The scheme a URL parser would resolve, with the characters parsers ignore removed. */
private fun iconScheme(candidate: String): String? {
    val stripped = candidate.filterNot { it.isWhitespace() || it == '\u0000' }
    val scheme = stripped.substringBefore(':', missingDelimiterValue = "")
    if (scheme.isEmpty() || !scheme.all { (it.code < 128 && it.isLetterOrDigit()) || it in "+-." }) return null

    return scheme.lowercase()
}

/** Whether an inline image declares an allowed media type, read whitespace-insensitively. */
private fun isAllowedIconDataUrl(candidate: String): Boolean {
    val mediaType = candidate.substringAfter(':')
        .split(',', ';')
        .first()
        .filterNot(Char::isWhitespace)
        .lowercase()

    return mediaType in ALLOWED_ICON_DATA_TYPES
}

/** The core's `is_display_unsafe`. */
private fun isDisplayUnsafe(codePoint: Int): Boolean =
    Character.isISOControl(codePoint) ||
        codePoint == 0x200B ||
        codePoint == 0x061C ||
        codePoint == 0x2028 || codePoint == 0x2029 ||
        codePoint in 0x202A..0x202E ||
        codePoint in 0x2066..0x2069 ||
        codePoint == 0xFEFF ||
        codePoint in 0xE0000..0xE007F

private fun chatIconError(message: String): Result<String?> = Result.failure(IllegalArgumentException(message))
