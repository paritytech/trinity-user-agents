package io.paritytech.polkadotapp.feature_dotns_api.presentation

import android.net.Uri
import android.webkit.MimeTypeMap
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import io.paritytech.polkadotapp.common.utils.notFoundResponse
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsUtils
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_dotns_api.domain.resolveToLocalFile
import kotlinx.coroutines.runBlocking
import timber.log.Timber
import java.io.ByteArrayInputStream
import java.io.File
import java.net.URLConnection

open class DotNsWebViewClient(
    private val dotNsResolver: DotNsResolver,
    private val dotNsTldProvider: DotNsTldProvider,
    private val servingHostResolver: DotNsServingHostResolver = DotNsServingHostResolver.Identity,
    // Stamped onto the main-frame document response so the caller can enforce document-level policy
    // (e.g. a Content-Security-Policy that forbids iframes) engine-side rather than by heuristics.
    private val mainDocumentResponseHeaders: Map<String, String> = emptyMap(),
) : WebViewClient() {
    override fun shouldInterceptRequest(
        view: WebView,
        request: WebResourceRequest
    ): WebResourceResponse? {
        val url = request.url

        Timber.d("Intercepting request for $url")

        val tld = runBlocking { dotNsTldProvider.getTldRetrying() }
        if (!DotNsUtils.isDotDomain(url, tld)) {
            Timber.d("Not dotNs domain: $url")

            return null
        }

        val requestPath = (url.path ?: "/").removePrefix("/")

        val resolveHostFile = resolveHostFile(url) ?: run {
            Timber.w("Archive root not resolved $url")
            return notFoundResponse()
        }
        val resolvedFile = resolveFile(resolveHostFile, requestPath)
            ?: spaFallbackFile(resolveHostFile, request)
            ?: run {
                Timber.w("File within archive not resolved $url")
                return notFoundResponse()
            }

        val mimeType = resolveMimeType(resolvedFile)

        Timber.d("Successfully resolved file for $url: ${resolvedFile.name}, mimeType=$mimeType")

        val stream = resolvedFile.inputStream()
        return if (request.isForMainFrame && mainDocumentResponseHeaders.isNotEmpty()) {
            WebResourceResponse(mimeType, "UTF-8", 200, "OK", mainDocumentResponseHeaders, stream)
        } else {
            WebResourceResponse(mimeType, "UTF-8", stream)
        }
    }

    fun resolveHostFile(uri: Uri?): File? {
        val host = uri?.host ?: return null

        return runBlocking {
            val servedHost = servingHostResolver.servingHostFor(host)
            dotNsResolver.resolveToLocalFile(servedHost)
                .getOrNull() // TODO: Bad ux. We have to ask user to reload the page
        }
    }

    /**
     * Resolves a request path to a file in the content directory.
     * 1. If contentDir is a plain file (single-file archive), serve it directly
     * 2. Try the exact path
     * 3. If no extension, try path/index.html (directory index)
     * 4. For root requests, try index.html
     */
    fun resolveFile(contentDir: File?, requestPath: String): File? {
        // Single-file archive — contentDir is a file, not a directory
        if (contentDir?.isFile == true) return contentDir

        // Root request, but it is a folder - try index.html
        if (requestPath.isEmpty()) {
            val index = File(contentDir, "index.html")
            if (index.exists()) return index
        }

        val exact = File(contentDir, requestPath)
        if (exact.exists() && exact.isFile) return exact

        // If no extension, try as directory with index.html
        if (!requestPath.contains('.')) {
            val dirIndex = File(contentDir, "$requestPath/index.html")
            if (dirIndex.exists()) return dirIndex
        }

        return null
    }

    /**
     * SPA fallback. Client-side routes (e.g. `/apps`) have no matching file in the archive — they
     * exist only in the app's JS router. When the WebView issues a hard main-frame request for such
     * a route (back across a cross-document boundary, refresh, or a shared deep link), serve the
     * archive's root `index.html` so the SPA boots and routes itself from `window.location`, mirroring
     * a web server rewriting unknown routes to `index.html`. Sub-resource requests still 404 so that
     * genuinely missing assets are not masked.
     */
    private fun spaFallbackFile(contentDir: File?, request: WebResourceRequest): File? {
        if (!request.isForMainFrame) return null
        return File(contentDir, "index.html").takeIf { it.exists() && it.isFile }
    }

    private fun resolveMimeType(file: File): String {
        return MimeTypeMap.getSingleton().getMimeTypeFromExtension(file.extension)
            ?: guessMimeFromContent(file)
            ?: "application/octet-stream"
    }

    private fun guessMimeFromContent(file: File): String? =
        file.inputStream().buffered().use { URLConnection.guessContentTypeFromStream(it) }

    private fun notFoundResponse(): WebResourceResponse {
        return WebResourceResponse(
            "text/plain",
            "UTF-8",
            ByteArrayInputStream("Not Found".toByteArray())
        ).apply {
            setStatusCodeAndReasonPhrase(404, "Not Found")
        }
    }
}
