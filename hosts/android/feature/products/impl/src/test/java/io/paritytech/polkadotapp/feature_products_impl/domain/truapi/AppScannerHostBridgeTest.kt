package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
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
    private val bridge = AppScannerHostBridge(scans, visible, TestCoroutineDispatchers(Dispatchers.Unconfined))

    @Test
    fun `an App off screen gets NotVisible and no viewfinder, a Worker is not checked`() = runTest {
        // The viewfinder would open over whatever the user is looking at. The
        // core already checked that a Worker's user tapped its card.
        assertEquals(HostScan.NotVisible, bridge.scanCode("greenmarket.dot", ProductExecutionKind.APP, request))
        coVerify(exactly = 0) { scans.ask(any()) }

        assertEquals(HostScan.Dismissed, bridge.scanCode("greenmarket.dot", ProductExecutionKind.WORKER, request))

        every { visible.isOnScreen("greenmarket.dot") } returns true
        assertEquals(HostScan.Dismissed, bridge.scanCode("greenmarket.dot", ProductExecutionKind.APP, request))
        coVerify(exactly = 2) { scans.ask(any()) }
    }
}
