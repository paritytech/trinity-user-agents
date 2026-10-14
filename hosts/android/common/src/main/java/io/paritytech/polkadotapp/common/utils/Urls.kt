package io.paritytech.polkadotapp.common.utils

import android.net.Uri
import android.util.Patterns
import androidx.core.net.toUri
import java.net.URI
import java.net.URL
import kotlin.let
import kotlin.text.removeSuffix
import kotlin.text.replace
import kotlin.text.startsWith

object Urls {
    const val HTTP_PREFIX = "http://"
    const val HTTPS_PREFIX = "https://"

    /**
     * @return normalized url in a form of protocol://host
     */
    fun normalizeUrl(url: String): String {
        val parsedUrl = URL(url)

        return "${parsedUrl.protocol}://${parsedUrl.host}"
    }

    fun normalizePath(url: String): String {
        return url.removeSuffix("/").let {
            URI.create(it).normalize().toString()
        }
    }

    fun hostOf(url: String): String {
        return URL(url).host
    }

    fun domainOf(url: String): String {
        return URL(url).authority
    }

    fun isValidWebUrl(url: String): Boolean {
        return Patterns.WEB_URL.matcher(url).matches()
    }

    /** Unlike [isValidWebUrl], accepts only absolute http(s) URLs with a host, and runs without Android. */
    fun isAbsoluteWebUrl(url: String): Boolean {
        val uri = runCatching { URI(url) }.getOrNull() ?: return false
        return uri.scheme?.lowercase() in WEB_SCHEMES && uri.host != null
    }

    fun ensureHasProtocolOrHttps(uri: Uri): Uri {
        return ensureHasProtocolOrHttps(uri.toString()).toUri()
    }

    fun ensureHasProtocolOrHttps(url: String): String {
        return when {
            url.startsWith(HTTPS_PREFIX) -> url
            url.startsWith(HTTP_PREFIX) -> url
            else -> "$HTTPS_PREFIX$url"
        }
    }

    fun ensureHttpsProtocol(url: String): String {
        return when {
            url.startsWith(HTTPS_PREFIX) -> url
            url.startsWith(HTTP_PREFIX) -> url.replace(HTTP_PREFIX, HTTPS_PREFIX)
            else -> "$HTTPS_PREFIX$url"
        }
    }

    fun ensureHttpsProtocol(url: Uri): Uri {
        return ensureHttpsProtocol(url.toString()).toUri()
    }

    private val WEB_SCHEMES = setOf("http", "https")
}
