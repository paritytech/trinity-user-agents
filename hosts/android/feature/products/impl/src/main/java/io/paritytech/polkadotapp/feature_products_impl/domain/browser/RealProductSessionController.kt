package io.paritytech.polkadotapp.feature_products_impl.domain.browser

import android.content.Context
import android.net.Uri
import android.webkit.WebView
import androidx.core.net.toUri
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.handleAndProcessOutcomeWithSystemFallback
import io.paritytech.polkadotapp.common.presentation.notification.AppNotifier
import io.paritytech.polkadotapp.common.presentation.notification.error
import io.paritytech.polkadotapp.common.presentation.screens.MessageDisplay
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.capitalize
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.domain.browser.BrowserSessionInfo
import io.paritytech.polkadotapp.feature_products_api.domain.browser.ProductSessionController
import io.paritytech.polkadotapp.feature_products_api.domain.browser.TabInfo
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toUrl
import io.paritytech.polkadotapp.feature_products_impl.data.repository.BrowserTabRepository
import io.paritytech.polkadotapp.feature_products_impl.data.repository.PersistedTab
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductRegistrar
import io.paritytech.polkadotapp.feature_products_impl.domain.product.gatewayUrlOf
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import java.net.URI
import javax.inject.Inject
import javax.inject.Singleton

private const val MAX_LIVE_TABS = 3

/**
 * App-lifetime owner of the product tabs and their live WebViews.
 *
 * There are exactly two mutable sources of truth — the list of [tabs] and the [activeTabId]. Everything the
 * UI observes ([activeTab], [webView], [loadProgress], [openTabs]) is *derived* from them (plus each tab's
 * own content and the resolved [productIcons]); nothing is pushed by hand. Commands ([openProduct],
 * [selectTab], [closeActiveTab], …) only mutate those two states, and the derived flows update themselves.
 * The [visitStack] rides alongside as unobserved bookkeeping so [goToPreviousTab] can undo focus changes.
 *
 * Each tab owns its WebView stack. At most [MAX_LIVE_TABS] stay alive (LRU); the rest "sleep" (WebView
 * destroyed, url kept) and rehydrate when selected. A tab's [CoroutineScope] drives its [HostApiSession]
 * disposal, so cancelling it destroys the WebView; the browser fragment only attaches, never destroys.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@Singleton
class RealProductSessionController @Inject constructor(
    private val tabSessionFactory: ProductTabSessionFactory,
    private val productRegistrar: ProductRegistrar,
    private val deepLinkHandler: DeepLinkHandler,
    private val appNotifier: AppNotifier,
    private val repository: BrowserTabRepository,
    private val productRepository: ProductRepository,
    private val ipfsContentLookup: IpfsContentLookup,
    @param:ApplicationContext private val context: Context,
    private val dispatchers: CoroutineDispatchers,
    private val dotNsTldProvider: DotNsTldProvider,
) : ProductSessionController {
    private val appScope = CoroutineScope(SupervisorJob() + dispatchers.main)

    private val tabs = MutableStateFlow<List<Tab>>(emptyList())
    private val activeTabId = MutableStateFlow<Long?>(null)

    // Tab ids in visit order, active one last. Back walks it down; re-visiting a tab moves it to the top
    // instead of duplicating it, so back never ping-pongs between two tabs.
    private val visitStack = ArrayDeque<Long>()

    // productId.value -> gateway url; tab badges read them as resolution fills each product's icon.
    private val productIcons: StateFlow<Map<String, String>> = productRepository.observeProducts()
        .map { products ->
            products.mapNotNull { product ->
                product.icon?.let { ipfsContentLookup.gatewayUrlOf(it) }?.let { product.id.value to it }
            }.toMap()
        }
        .stateIn(appScope, SharingStarted.Eagerly, emptyMap())

    private var nextId = 0L

    private val activeTabFlow: Flow<Tab?> =
        combine(tabs, activeTabId) { list, id -> list.firstOrNull { it.id == id } }

    override val activeTab: StateFlow<BrowserSessionInfo?> = activeTabFlow.flatMapLatest { it?.sessionInfo ?: flowOf(null) }
        .stateIn(appScope, SharingStarted.Eagerly, null)

    override val webView: StateFlow<WebView?> = activeTabFlow.flatMapLatest { it?.webView ?: flowOf(null) }
        .stateIn(appScope, SharingStarted.Eagerly, null)

    override val loadProgress: StateFlow<DotNsLoadProgress> = activeTabFlow.flatMapLatest { it?.loadProgress ?: flowOf(DotNsLoadProgress.Idle) }
        .stateIn(appScope, SharingStarted.Eagerly, DotNsLoadProgress.Idle)

    override val openTabs: StateFlow<List<TabInfo>> = combine(tabs, activeTabId, productIcons, ::TabsSnapshot)
        .flatMapLatest { it.toTabInfos() }
        .stateIn(appScope, SharingStarted.Eagerly, emptyList())

    init {
        appScope.launch { restore() }
    }

    /** Focus the tab already showing [productId] (one tab per product), else create one. */
    override fun openProduct(productId: ProductId) = openTab(productId.toUrl())

    override fun openUrl(url: String) = openTab(url)

    override fun selectTab(id: Long) {
        tabs.value.firstOrNull { it.id == id }?.let(::activate)
    }

    override fun goToPreviousTab(): Boolean {
        val previousId = visitStack.getOrNull(visitStack.lastIndex - 1) ?: return false
        val previous = tabs.value.firstOrNull { it.id == previousId } ?: return false
        visitStack.removeLast()
        activate(previous)
        return true
    }

    override fun resetVisitHistory() {
        visitStack.clear()
    }

    override fun closeActiveTab() {
        activeTabId.value?.let(::closeTab)
    }

    override fun ensureActiveLive() {
        val tab = activeTabOrNull() ?: return
        tab.lastActive = now()
        tab.ensureLive()
        enforceLiveLimit()
        persist(tab)
    }

    // Focus the same-product tab (one per product), else open a new one from [url] (keeping its path).
    private fun openTab(url: String) {
        val tld = dotNsTldProvider.currentTldOrNull()
        val existing = tabs.value.firstOrNull { productKey(it.url.value, tld) == productKey(url, tld) }
        val tab = existing ?: Tab(nextId++, url).also { new -> tabs.update { it + new } }
        activate(tab)
    }

    private fun activate(tab: Tab) {
        tab.lastActive = now()
        tab.ensureLive()
        activeTabId.value = tab.id
        pushVisit(tab.id)
        enforceLiveLimit()
        persist(tab)
    }

    private fun pushVisit(id: Long) {
        visitStack.remove(id)
        visitStack.addLast(id)
    }

    override fun closeTab(id: Long) {
        val tab = tabs.value.firstOrNull { it.id == id } ?: return
        tab.sleep()
        tabs.update { it - tab }
        visitStack.remove(id)
        deletePersisted(id)
        if (activeTabId.value != id) return
        val next = tabs.value.maxByOrNull { it.lastActive }
        if (next == null) activeTabId.value = null else activate(next)
    }

    private fun enforceLiveLimit() {
        val live = tabs.value.filter { it.isLive }.sortedBy { it.lastActive }
        val excess = live.size - MAX_LIVE_TABS
        if (excess <= 0) return
        live.take(excess).forEach { if (it.id != activeTabId.value) it.sleep() }
    }

    private fun activeTabOrNull(): Tab? = tabs.value.firstOrNull { it.id == activeTabId.value }

    private inner class Tab(val id: Long, initialUrl: String) {
        val url = MutableStateFlow(initialUrl)
        val title = MutableStateFlow("")
        val loadProgress = MutableStateFlow<DotNsLoadProgress>(DotNsLoadProgress.Idle)
        val webView = MutableStateFlow<WebView?>(null)
        var lastActive = 0L

        private var scope: CoroutineScope? = null
        val isLive: Boolean get() = scope != null

        // What the active-tab UI shows: reacts to this tab's own url/title/progress.
        val sessionInfo: Flow<BrowserSessionInfo> = combine(url, title, loadProgress, ::buildInfo)

        fun ensureLive() {
            if (isLive) return
            val tabScope = CoroutineScope(appScope.coroutineContext + SupervisorJob())
            scope = tabScope
            startSession(tabScope)
        }

        fun sleep() {
            scope?.cancel()
            scope = null
            webView.value = null
            loadProgress.value = DotNsLoadProgress.Idle
        }

        fun info(activeId: Long?, icons: Map<String, String>): Flow<TabInfo> =
            combine(url, title) { url, title ->
                TabInfo(
                    id = id,
                    title = displayTitle(url, title),
                    host = host(url),
                    iconUrl = productKey(url, dotNsTldProvider.currentTldOrNull()).let(icons::get),
                    isActive = id == activeId,
                )
            }

        private fun startSession(scope: CoroutineScope) {
            val provider = tabSessionFactory.create(
                url = url.value,
                scope = scope,
                onDeeplink = { launchDeeplink(it, scope) },
            )

            provider.addOnPageStartedListener { url.value = it }
            provider.addOnPageFinishedListener {
                title.value = webView.value?.title.orEmpty()
                persist(this)
            }

            provider.webViews().onEach { webView.value = it }.launchIn(scope)
            provider.loadProgress.onEach { loadProgress.value = it }.launchIn(scope)

            url.mapNotNull { rawUrl ->
                dotNsTldProvider.getTld().getOrNull()
                    ?.let { tld -> ProductId.fromUrl(rawUrl.toUri(), tld).getOrNull() }
            }
                .onEach { productRegistrar.ensureRegistered(it) }
                .launchIn(scope)
        }
    }

    // Restore persisted tabs as sleeping (url + title only), all of them inactive: a tab rehydrates when the
    // user picks it from the tab bar.
    private suspend fun restore() {
        if (tabs.value.isNotEmpty()) return
        val persisted = repository.loadAll()
        if (persisted.isEmpty()) return
        val tld = dotNsTldProvider.getTld().getOrNull()
        // One tab per product: collapse persisted duplicates, keeping the most recently active.
        val deduped = persisted.groupBy { productKey(it.url, tld) }.map { (_, group) -> group.maxBy { it.lastActive } }
        (persisted - deduped.toSet()).forEach { deletePersisted(it.id) }

        tabs.value = deduped.map { p ->
            Tab(p.id, p.url).apply {
                title.value = p.title
                lastActive = p.lastActive
            }
        }
        nextId = deduped.maxOf { it.id } + 1
    }

    private fun persist(tab: Tab) {
        if (tab !in tabs.value) return
        val snapshot = PersistedTab(tab.id, tab.url.value, tab.title.value, position = tab.id.toInt(), tab.lastActive)
        appScope.launch { repository.save(snapshot) }
    }

    private fun deletePersisted(id: Long) {
        appScope.launch { repository.delete(id) }
    }

    // --- Helpers ---

    private fun buildInfo(url: String, title: String, progress: DotNsLoadProgress) = BrowserSessionInfo(
        title = displayTitle(url, title),
        host = host(url),
        url = url,
        isLoading = progress.isLoading(),
        loadFraction = (progress as? DotNsLoadProgress.Downloading)?.fraction,
    )

    private fun displayTitle(url: String, title: String): String =
        title.ifEmpty { host(url)?.capitalize() ?: url }

    private fun host(url: String): String? = runCatching { URI(url).host }.getOrNull()

    // Groups tabs by product (one tab per product); falls back to the raw url for non-product tabs.
    private fun productKey(url: String, tld: DotNsTld?): String =
        tld?.let { ProductId.fromUrl(url.toUri(), it).getOrNull()?.value } ?: url

    private fun now(): Long = System.currentTimeMillis()

    private fun DotNsLoadProgress.isLoading(): Boolean =
        this is DotNsLoadProgress.Resolving ||
            this is DotNsLoadProgress.Downloading ||
            this is DotNsLoadProgress.Unpacking

    // App-level message surface for deeplink fallbacks (the controller has no UI of its own).
    private val messageDisplay = object : MessageDisplay {
        override fun showMessage(text: String) = appNotifier.error(text)
    }

    private fun launchDeeplink(data: Uri, scope: CoroutineScope) {
        scope.launch {
            with(ComputationalScope(this)) {
                with(messageDisplay) {
                    with(context) {
                        deepLinkHandler.handleAndProcessOutcomeWithSystemFallback(data)
                    }
                }
            }
        }
    }

    private data class TabsSnapshot(val tabs: List<Tab>, val activeId: Long?, val icons: Map<String, String>)

    // Each tab exposes its own TabInfo flow; combine them into one list that re-emits when any tab's title
    // (or the active id / icons captured in the snapshot) changes.
    private fun TabsSnapshot.toTabInfos(): Flow<List<TabInfo>> =
        if (tabs.isEmpty()) flowOf(emptyList())
        else combine(tabs.map { it.info(activeId, icons) }) { it.asList() }
}
