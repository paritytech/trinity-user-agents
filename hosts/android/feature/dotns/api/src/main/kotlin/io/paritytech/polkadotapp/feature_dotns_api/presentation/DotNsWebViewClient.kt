package io.paritytech.polkadotapp.feature_dotns_api.presentation

import android.net.Uri
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import io.paritytech.polkadotapp.common.utils.notFoundResponse
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsUtils
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_dotns_api.domain.resolveToLocalFile
import kotlinx.coroutines.runBlocking
import timber.log.Timber
import uniffi.truapi.parseProductLink
import java.io.File

open class DotNsWebViewClient(
    private val dotNsResolver: DotNsResolver,
    private val dotNsTldProvider: DotNsTldProvider,
    private val servingHostResolver: DotNsServingHostResolver = DotNsServingHostResolver.Identity,
    // Stamped onto the main-frame document response so the caller can enforce document-level policy
    // (e.g. a Content-Security-Policy that forbids iframes) engine-side rather than by heuristics.
    private val mainDocumentResponseHeaders: Map<String, String> = emptyMap(),
) : WebViewClient() {
    private val archive = ArchiveContent(DiskArchiveFiles, mainDocumentResponseHeaders)

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

        val root = resolveHostFile(url) ?: run {
            Timber.w("Archive root not resolved $url")
            return notFoundResponse()
        }
        return archive.respond(root.path, request)
    }

    fun resolveHostFile(uri: Uri?): File? {
        val host = uri?.host ?: return null

        return runBlocking {
            runCancellableCatching {
                val tld = dotNsTldProvider.getTld().getOrThrow()
                val canonicalHost = parseProductLink(host, tld.value)?.identifier?.value ?: host
                val servedHost = servingHostResolver.servingHostFor(canonicalHost).getOrThrow()
                dotNsResolver.resolveToLocalFile(servedHost).getOrThrow()
            }.onFailure { Timber.w(it, "Archive root not resolved for %s", host) }.getOrNull()
        }
    }
}
