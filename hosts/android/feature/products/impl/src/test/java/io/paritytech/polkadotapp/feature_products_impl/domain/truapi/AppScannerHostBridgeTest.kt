package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.CodeFormat
import uniffi.truapi.HostScan
import uniffi.truapi.HostScannerScanRequest
import uniffi.truapi.ProductExecutionKind

class AppScannerHostBridgeTest {
    private val request = HostScannerScanRequest(listOf(CodeFormat.QR), null, null)
    private val scans = mockk<TrUAPIProductScans> { coEvery { ask(any()) } returns HostScan.Dismissed }
    private val visible = mockk<VisibleProducts> { every { isOnScreen(any()) } returns false }
    private val app = mockk<AppLifecycleObserver> { every { getCurrentState() } returns AppLifecycleState.BACKGROUND }
    private val bridge = AppScannerHostBridge(scans, visible, app, TestCoroutineDispatchers(Dispatchers.Unconfined))

    private suspend fun scan(kind: ProductExecutionKind) = bridge.scanCode("greenmarket.dot", kind, request)

    @Test
    fun `a product the user cannot see gets NotVisible and no viewfinder`() = runTest {
        // The viewfinder would open over whatever the user is looking at. A Worker has no page, so
        // it needs the app in front. The core already checked that its user tapped its card.
        assertEquals(HostScan.NotVisible, scan(ProductExecutionKind.APP))
        assertEquals(HostScan.NotVisible, scan(ProductExecutionKind.WORKER))
        coVerify(exactly = 0) { scans.ask(any()) }

        every { app.getCurrentState() } returns AppLifecycleState.FOREGROUND
        assertEquals(HostScan.NotVisible, scan(ProductExecutionKind.APP))
        assertEquals(HostScan.Dismissed, scan(ProductExecutionKind.WORKER))

        every { visible.isOnScreen("greenmarket.dot") } returns true
        assertEquals(HostScan.Dismissed, scan(ProductExecutionKind.APP))
        coVerify(exactly = 2) { scans.ask(any()) }
    }
}
