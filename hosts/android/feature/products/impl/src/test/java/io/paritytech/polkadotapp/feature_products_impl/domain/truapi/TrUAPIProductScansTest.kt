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
import kotlinx.coroutines.async
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
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

    private fun TestScope.scansOnTestClock() =
        TrUAPIProductScans(router, visible, contextManager, TestCoroutineDispatchers(UnconfinedTestDispatcher(testScheduler)))

    private suspend fun TrUAPIProductScans.scan(kind: ProductExecutionKind) = scanCode("greenmarket.dot", kind, request)

    private fun appInFront() {
        val resumed = mockk<ComponentActivity> { every { lifecycle.currentState } returns Lifecycle.State.RESUMED }
        every { contextManager.getActivity() } returns resumed
    }

    @Test
    fun `a product the user cannot see gets NotVisible and no viewfinder`() = runTest {
        val scans = scansOnTestClock()
        assertEquals(HostScan.NotVisible, scans.scan(ProductExecutionKind.APP))
        assertEquals(HostScan.NotVisible, scans.scan(ProductExecutionKind.WORKER))

        appInFront()
        assertEquals(HostScan.NotVisible, scans.scan(ProductExecutionKind.APP))
        coVerify(exactly = 0) { router.openTrUAPIProductScan() }
    }

    @Test
    fun `a scan waiting behind another is checked when its turn comes`() = runTest {
        // By then the user may have left the page that asked.
        val scans = scansOnTestClock()
        appInFront()
        every { visible.isOnScreen("greenmarket.dot") } returns true
        val first = async(start = CoroutineStart.UNDISPATCHED) { scans.scan(ProductExecutionKind.WORKER) }
        val firstPrompt = checkNotNull(scans.current).also { it.markShown() }
        val waiting = async(start = CoroutineStart.UNDISPATCHED) { scans.scan(ProductExecutionKind.APP) }

        every { visible.isOnScreen("greenmarket.dot") } returns false
        firstPrompt.answer(HostScan.Dismissed)

        assertEquals(HostScan.Dismissed, first.await())
        assertEquals(HostScan.NotVisible, waiting.await())
        coVerify(exactly = 1) { router.openTrUAPIProductScan() }
    }

    @Test
    fun `a page getting focus back from the closing scanner can scan again`() = runTest {
        // The product hears the last answer before the closing sheet hands focus back to its page.
        val scans = scansOnTestClock()
        every { visible.isOnScreen("greenmarket.dot") } returnsMany listOf(false, false, true)
        val scanned = async(start = CoroutineStart.UNDISPATCHED) { scans.scan(ProductExecutionKind.APP) }

        advanceTimeBy(200)
        runCurrent()
        checkNotNull(scans.current).apply {
            markShown()
            answer(HostScan.Dismissed)
        }

        assertEquals(HostScan.Dismissed, scanned.await())
    }
}
