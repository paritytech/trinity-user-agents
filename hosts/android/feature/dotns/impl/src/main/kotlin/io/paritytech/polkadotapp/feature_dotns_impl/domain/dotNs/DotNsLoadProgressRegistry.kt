package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import java.util.concurrent.ConcurrentHashMap

/**
 * Single source of truth for per-domain [DotNsLoadProgress]. The resolver drives transitions as it
 * downloads and unpacks; [observe] hands the host UI a hot flow it can subscribe to before, during,
 * or after a resolution.
 */
internal class DotNsLoadProgressRegistry {
    private val flows = ConcurrentHashMap<String, MutableStateFlow<DotNsLoadProgress>>()

    fun observe(domain: String): StateFlow<DotNsLoadProgress> = flowFor(domain).asStateFlow()

    fun markResolving(domain: String) {
        flowFor(domain).value = DotNsLoadProgress.Resolving
    }

    fun markDownloadProgress(domain: String, downloaded: Long, total: Long?) {
        val fraction = total?.takeIf { it > 0 }?.let { (downloaded.toFloat() / it).coerceIn(0f, 1f) }
        flowFor(domain).value = DotNsLoadProgress.Downloading(fraction)
    }

    fun markUnpacking(domain: String) {
        flowFor(domain).value = DotNsLoadProgress.Unpacking
    }

    fun markCompleted(domain: String) {
        flowFor(domain).value = DotNsLoadProgress.Completed
    }

    fun markFailed(domain: String, cause: Throwable) {
        flowFor(domain).value = DotNsLoadProgress.Failed(cause)
    }

    fun clear() {
        flows.clear()
    }

    private fun flowFor(domain: String): MutableStateFlow<DotNsLoadProgress> =
        flows.getOrPut(domain) { MutableStateFlow(DotNsLoadProgress.Idle) }
}
