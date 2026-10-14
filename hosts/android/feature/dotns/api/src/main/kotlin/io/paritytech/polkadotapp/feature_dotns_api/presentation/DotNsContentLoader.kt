package io.paritytech.polkadotapp.feature_dotns_api.presentation

import android.net.Uri
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf

/**
 * Per-WebView-session decorator over [DotNsResolver] that tracks the domain currently being
 * resolved and exposes its load progress.
 *
 * Because it *is* a [DotNsResolver] (delegating everything else), it drops into the existing
 * [DotNsWebViewClient] constructor slot unchanged — `resolveToLocalFile`/`resolveToLocalUri` calls
 * made by the client record the requested domain as a side effect, and the host UI observes
 * [loadProgress] without the client knowing about it.
 */
class DotNsContentLoader(
    private val delegate: DotNsResolver
) : DotNsResolver by delegate {
    private val lastRequestedDomain = MutableStateFlow<String?>(null)

    @OptIn(ExperimentalCoroutinesApi::class)
    val loadProgress: Flow<DotNsLoadProgress> = lastRequestedDomain
        .flatMapLatest { domain ->
            if (domain == null) flowOf(DotNsLoadProgress.Idle) else delegate.getProgressByDomain(domain)
        }

    override suspend fun resolveToLocalUri(dotNsName: String): Result<Uri> {
        lastRequestedDomain.value = dotNsName
        return delegate.resolveToLocalUri(dotNsName)
    }
}
