package io.paritytech.polkadotapp.feature_products_impl.domain.permissions

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.update
import javax.inject.Inject
import javax.inject.Singleton

/** Process callback fanout, independent of the runtime/repository dependency graph. */
@Singleton
class PermissionAuthorizationChanges @Inject constructor() {
    private val revisions = MutableStateFlow<Map<String, Long>>(emptyMap())

    fun changed(productId: String) {
        val id = bareProductLabel(productId)
        revisions.update { it + (id to ((it[id] ?: 0L) + 1L)) }
    }

    fun revision(productId: String): Long = revisions.value[bareProductLabel(productId)] ?: 0L

    fun observe(productId: String) = revisions.map { it[bareProductLabel(productId)] ?: 0L }.distinctUntilChanged()
}
