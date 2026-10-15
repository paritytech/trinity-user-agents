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
import java.util.concurrent.atomic.AtomicBoolean

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
 * newer content. The session picks up newer content for a domain it has not loaded yet, and for the next document
 * after [pickUpLatestOnNextDocument].
 */
class DotNsContentLoader(
    private val delegate: DotNsResolver
) : DotNsResolver by delegate {
    private val lastRequestedDomain = MutableStateFlow<String?>(null)

    private val documentContent = ConcurrentHashMap<String, Uri>()

    private val pickUpLatest = AtomicBoolean(false)

    @OptIn(ExperimentalCoroutinesApi::class)
    val loadProgress: Flow<DotNsLoadProgress> = lastRequestedDomain
        .flatMapLatest { domain ->
            if (domain == null) flowOf(DotNsLoadProgress.Idle) else delegate.getProgressByDomain(domain)
        }

    override suspend fun resolveToLocalUri(dotNsName: String): Result<Uri> {
        lastRequestedDomain.value = dotNsName
        return documentContent[dotNsName]?.let { Result.success(it) } ?: delegate.resolveToLocalUri(dotNsName)
    }

    override suspend fun resolveLatestToLocalUri(dotNsName: String): Result<Uri> {
        lastRequestedDomain.value = dotNsName
        val isPickingUpLatest = pickUpLatest.getAndSet(false)
        documentContent[dotNsName]?.takeUnless { isPickingUpLatest }?.let { return Result.success(it) }
        return delegate.resolveLatestToLocalUri(dotNsName).onSuccess { documentContent[dotNsName] = it }
    }

    /** Moves the domain of the next document to its latest content, for a reload the user agreed to. */
    fun pickUpLatestOnNextDocument() {
        pickUpLatest.set(true)
    }

    /** Whether [dotNsName] has newer content than the documents of this session were loaded from. */
    suspend fun isDocumentOutdated(dotNsName: String): Result<Boolean> {
        val content = documentContent[dotNsName] ?: return Result.success(false)
        return delegate.isOutdated(dotNsName, content)
    }
}
