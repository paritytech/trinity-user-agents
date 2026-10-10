package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import androidx.lifecycle.Lifecycle
import io.parity.truapi.ScannerHostBridge
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import uniffi.truapi.HostScan
import uniffi.truapi.HostScannerScanRequest
import uniffi.truapi.ProductExecutionKind
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

/** What the viewfinder shows: who is asking, and what they accept. */
class ProductScanRequest(
    val productId: String,
    val executionKind: ProductExecutionKind,
    val request: HostScannerScanRequest,
)

/**
 * Serves `scanner.scan`: opens the viewfinder for a product the user can see and waits for the
 * code they scanned. A Worker has no page, so the app must be in front. The core already checked
 * that its user tapped its card.
 */
@Singleton
class TrUAPIProductScans @Inject constructor(
    private val productsRouter: ProductsRouter,
    private val visibleProducts: VisibleProducts,
    private val contextManager: ContextManager,
    private val dispatchers: CoroutineDispatchers,
) : TrUAPIPrompts<ProductScanRequest, HostScan>(unanswered = HostScan.Dismissed, notShown = HostScan.NotVisible),
    ScannerHostBridge {
    override suspend fun scanCode(
        productId: String,
        executionKind: ProductExecutionKind,
        request: HostScannerScanRequest,
    ): HostScan = ask(ProductScanRequest(productId, executionKind, request))

    override suspend fun canShow(question: ProductScanRequest): Boolean = withContext(dispatchers.main) {
        when (question.executionKind) {
            ProductExecutionKind.WORKER ->
                contextManager.getActivity()?.lifecycle?.currentState?.isAtLeast(Lifecycle.State.RESUMED) == true
            ProductExecutionKind.APP, ProductExecutionKind.WIDGET -> pageOnScreen(question.productId)
        }
    }

    override suspend fun open() = productsRouter.openTrUAPIProductScan()

    override suspend fun close() = productsRouter.closeTrUAPIProductScan()

    // A host screen closing over the page, such as the last scanner, holds focus for a moment.
    private suspend fun pageOnScreen(productId: String) = withTimeoutOrNull(FOCUS_RETURN_TIMEOUT) {
        while (!visibleProducts.isOnScreen(productId)) delay(FOCUS_POLL_INTERVAL)
    } != null

    private companion object {
        val FOCUS_RETURN_TIMEOUT = 1.seconds
        val FOCUS_POLL_INTERVAL = 50.milliseconds
    }
}
