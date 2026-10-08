package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.ScannerHostBridge
import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.withContext
import uniffi.truapi.HostScan
import uniffi.truapi.HostScannerScanRequest
import uniffi.truapi.ProductExecutionKind
import javax.inject.Inject
import javax.inject.Singleton

/** What the viewfinder shows: who is asking, and what they accept. */
class ProductScanRequest(
    val productId: String,
    val request: HostScannerScanRequest,
)

/** Opens the viewfinder for a product and waits for the code the user scanned. */
@Singleton
class TrUAPIProductScans @Inject constructor(
    private val productsRouter: ProductsRouter,
) : TrUAPIPrompts<ProductScanRequest, HostScan>(unanswered = HostScan.Dismissed) {
    override suspend fun open() = productsRouter.openTrUAPIProductScan()

    override suspend fun close() = productsRouter.closeTrUAPIProductScan()
}

/**
 * Serves `scanner.scan`. A Worker has no page, so it needs the app in front. The core already
 * checked that its user tapped its card.
 */
class AppScannerHostBridge @Inject constructor(
    private val scans: TrUAPIProductScans,
    private val visibleProducts: VisibleProducts,
    private val appLifecycle: AppLifecycleObserver,
    private val dispatchers: CoroutineDispatchers,
) : ScannerHostBridge {
    override suspend fun scanCode(
        productId: String,
        executionKind: ProductExecutionKind,
        request: HostScannerScanRequest,
    ): HostScan {
        val onScreen = when (executionKind) {
            ProductExecutionKind.WORKER -> appLifecycle.getCurrentState() == AppLifecycleState.FOREGROUND
            else -> withContext(dispatchers.main) { visibleProducts.isOnScreen(productId) }
        }
        return if (onScreen) scans.ask(ProductScanRequest(productId, request)) else HostScan.NotVisible
    }
}
