package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import androidx.activity.ComponentActivity
import androidx.lifecycle.Lifecycle
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.CodeFormat
import uniffi.truapi.HostScan
import uniffi.truapi.HostScannerScanRequest
import uniffi.truapi.ProductExecutionKind

class TrUAPIProductScansTest {
    private val request = HostScannerScanRequest(listOf(CodeFormat.QR), null, null)
    private val router = mockk<ProductsRouter>(relaxUnitFun = true)
    private val visible = mockk<VisibleProducts> { every { isOnScreen(any()) } returns false }
    private val contextManager = mockk<ContextManager> { every { getActivity() } returns null }
    private val scans = TrUAPIProductScans(router, visible, contextManager, TestCoroutineDispatchers(Dispatchers.Unconfined))

    private suspend fun scan(kind: ProductExecutionKind) = scans.scanCode("greenmarket.dot", kind, request)

    private fun appInFront() {
        val resumed = mockk<ComponentActivity> { every { lifecycle.currentState } returns Lifecycle.State.RESUMED }
        every { contextManager.getActivity() } returns resumed
    }

    @Test
    fun `a product the user cannot see gets NotVisible and no viewfinder`() = runTest {
        assertEquals(HostScan.NotVisible, scan(ProductExecutionKind.APP))
        assertEquals(HostScan.NotVisible, scan(ProductExecutionKind.WORKER))

        appInFront()
        assertEquals(HostScan.NotVisible, scan(ProductExecutionKind.APP))
        coVerify(exactly = 0) { router.openTrUAPIProductScan() }
    }

    @Test
    fun `a scan waiting behind another is checked when its turn comes`() = runTest {
        // By then the user may have left the page that asked.
        appInFront()
        every { visible.isOnScreen("greenmarket.dot") } returns true
        val first = async(start = CoroutineStart.UNDISPATCHED) { scan(ProductExecutionKind.WORKER) }
        val firstPrompt = checkNotNull(scans.current).also { it.markShown() }
        val waiting = async(start = CoroutineStart.UNDISPATCHED) { scan(ProductExecutionKind.APP) }

        every { visible.isOnScreen("greenmarket.dot") } returns false
        firstPrompt.answer(HostScan.Dismissed)

        assertEquals(HostScan.Dismissed, first.await())
        assertEquals(HostScan.NotVisible, waiting.await())
        coVerify(exactly = 1) { router.openTrUAPIProductScan() }
    }
}
