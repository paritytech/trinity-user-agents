package io.paritytech.polkadotapp.common.data.network

import io.paritytech.polkadotapp.common.data.memory.SingleValueCache
import io.paritytech.polkadotapp.common.utils.mapError
import kotlinx.coroutines.runBlocking
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.Interceptor
import okhttp3.Response
import java.io.IOException

/**
 * Rewrites requests targeting the [placeholderUrlName] host onto the URL produced by [resolveUrl]
 * (typically read from remote config), keeping its path as a prefix. Requests with a different host
 * pass through untouched, so this can safely live on a shared client.
 *
 * Install it as the first interceptor so logging and other interceptors observe the final URL. It
 * runs on OkHttp's background dispatcher, so resolving via [runBlocking] is safe here. The resolved
 * URL is cached for the app session ([SingleValueCache]) since this is a per-request hot path; a
 * failed resolution is not cached and is retried on the next request.
 */
class OverrideBaseUrlInterceptor(
    private val placeholderUrlName: String,
    private val resolveUrl: suspend () -> String,
) : Interceptor {
    private val baseUrlCache = SingleValueCache {
        runCatching { resolveUrl() }
            .mapError { IOException("Failed to resolve base URL for $placeholderUrlName", it) }
            .getOrThrow()
            .toHttpUrl()
    }

    override fun intercept(chain: Interceptor.Chain): Response {
        val request = chain.request()
        if (request.url.host != placeholderUrlName) {
            return chain.proceed(request)
        }

        val baseUrl: HttpUrl = runBlocking { baseUrlCache() }

        val rewrittenUrl = request.url.newBuilder()
            .scheme(baseUrl.scheme)
            .host(baseUrl.host)
            .port(baseUrl.port)
            .encodedPath(baseUrl.encodedPath.removeSuffix("/") + request.url.encodedPath)
            .build()

        return chain.proceed(request.newBuilder().url(rewrittenUrl).build())
    }
}
