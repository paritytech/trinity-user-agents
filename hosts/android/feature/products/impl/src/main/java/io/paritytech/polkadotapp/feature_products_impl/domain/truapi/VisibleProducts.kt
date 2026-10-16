package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.view.View
import java.lang.ref.WeakReference
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Which products the user is looking at: those with a page shown in the focused window. A tab in
 * the background, a page under a host sheet, or an app in the background is not on screen.
 */
@Singleton
class VisibleProducts @Inject constructor() {
    private val pages = mutableListOf<Pair<String, WeakReference<View>>>()

    fun track(productId: String, page: View) = synchronized(pages) {
        pages.removeAll { it.second.get() == null }
        pages += productId to WeakReference(page)
    }

    /** Reads view state, so call it on the main thread. */
    fun isOnScreen(productId: String): Boolean = synchronized(pages) {
        pages.any { (id, page) ->
            id == productId && page.get()?.let { it.isAttachedToWindow && it.isShown && it.hasWindowFocus() } == true
        }
    }
}
