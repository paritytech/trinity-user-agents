package io.paritytech.polkadotapp.feature_products_api.domain.browser

import android.webkit.WebView
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import kotlinx.coroutines.flow.StateFlow

/**
 * App-wide runtime of the product tabs. Each tab owns its own WebView (bounded live pool, the rest
 * rehydrate from url). The browser presentation attaches the active tab's WebView; other features open
 * products through it.
 */
interface ProductSessionController {
    /** Display snapshot of the currently active tab, or null when there are no tabs. */
    val activeTab: StateFlow<BrowserSessionInfo?>

    /** All open tabs, for the tab bar. */
    val openTabs: StateFlow<List<TabInfo>>

    /** The active tab's live WebView (attached by the browser screen), or null while none is live. */
    val webView: StateFlow<WebView?>

    /** Load progress of the active tab's content. */
    val loadProgress: StateFlow<DotNsLoadProgress>

    /** Focus the tab already showing [productId] (one tab per product), otherwise create one. */
    fun openProduct(productId: ProductId)

    /** Like [openProduct] but from a raw url (keeps its path); used when only a url is known. */
    fun openUrl(url: String)

    /** Focus an already-open tab by its [id] (from [openTabs]). */
    fun selectTab(id: Long)

    /**
     * Focus the tab visited before the active one, dropping the active one from the visit history (the tab
     * itself stays open). Returns false when there is nothing left to go back to — the caller then leaves
     * the browser.
     */
    fun goToPreviousTab(): Boolean

    /**
     * Forget the visit history (the tabs stay open). The browser calls this when its screen is left, so the
     * next tab opened starts a fresh back chain instead of walking back into an earlier visit's tabs.
     */
    fun resetVisitHistory()

    /** Rehydrate the active tab's WebView if it is asleep, so the browser screen has one to attach. */
    fun ensureActiveLive()

    /** Close the active tab (and fall back to the most recent remaining tab, if any). */
    fun closeActiveTab()

    /**
     * Close the tab with the given [id]. Closing the active one falls back to the most recent remaining tab,
     * if any; closing any other tab leaves the active one untouched.
     */
    fun closeTab(id: Long)
}
