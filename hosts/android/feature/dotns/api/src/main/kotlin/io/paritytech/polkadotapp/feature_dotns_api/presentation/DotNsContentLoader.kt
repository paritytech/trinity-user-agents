package io.paritytech.polkadotapp.feature_dotns_api.presentation

import android.net.Uri
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import java.util.concurrent.ConcurrentHashMap

/**
 * Per-WebView-session decorator over [DotNsResolver] that tracks the domain currently being
 * resolved and exposes its load progress.
 *
 * Because it *is* a [DotNsResolver] (delegating everything else), it drops into the existing
 * [DotNsWebViewClient] constructor slot unchanged — `resolveToLocalFile`/`resolveToLocalUri` calls
 * made by the client record the requested domain as a side effect, and the host UI observes
 * [loadProgress] without the client knowing about it.
 *
 * A page keeps the content its document was loaded from: its later requests, and later documents of the same domain
 * (a link in a multi-page product, `location.reload()`), are served from there even after the domain moves on to
 * newer content. The session picks up the latest content for a domain it has not loaded yet, and for every domain
 * after [pickUpLatest].
 */
class DotNsContentLoader(
    private val delegate: DotNsResolver
) : DotNsResolver by delegate {
    private val lastRequestedDomain = MutableStateFlow<String?>(null)

    private val documentContent = ConcurrentHashMap<String, Uri>()

    @OptIn(ExperimentalCoroutinesApi::class)
    val loadProgress: Flow<DotNsLoadProgress> = lastRequestedDomain
        .flatMapLatest { domain ->
            if (domain == null) flowOf(DotNsLoadProgress.Idle) else delegate.getProgressByDomain(domain)
        }

    override suspend fun resolveToLocalUri(dotNsName: String): Result<Uri> {
        lastRequestedDomain.value = dotNsName
        documentContent[dotNsName]?.let { return Result.success(it) }
        return delegate.resolveLatestToLocalUri(dotNsName).onSuccess { documentContent[dotNsName] = it }
    }

    /** Moves every domain of this session to its latest content, for a reload the user agreed to. */
    fun pickUpLatest() {
        documentContent.clear()
    }

    /** Whether [dotNsName] has newer content than the documents of this session were loaded from. */
    suspend fun isDocumentOutdated(dotNsName: String): Result<Boolean> {
        val content = documentContent[dotNsName] ?: return Result.success(false)
        return delegate.isOutdated(dotNsName, content)
    }
}
