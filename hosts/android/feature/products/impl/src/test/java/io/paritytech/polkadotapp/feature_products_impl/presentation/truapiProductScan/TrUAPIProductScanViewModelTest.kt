package io.paritytech.polkadotapp.feature_products_impl.presentation.truapiProductScan

import com.google.mlkit.vision.barcode.common.Barcode
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductScanRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIProductScans
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIPrompt
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import uniffi.truapi.CodeFormat
import uniffi.truapi.HostScan
import uniffi.truapi.HostScannerScanRequest
import uniffi.truapi.ProductExecutionKind

@OptIn(ExperimentalCoroutinesApi::class)
class TrUAPIProductScanViewModelTest {
    private val router = mockk<ProductsRouter>(relaxed = true)
    private val receipt = "https://greenmarket.example/r/BAG6"

    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun scan(): Pair<TrUAPIPrompt<ProductScanRequest, HostScan>, TrUAPIProductScanViewModel> {
        val request = HostScannerScanRequest(listOf(CodeFormat.QR), "https://greenmarket.example/r/", null)
        val prompt = TrUAPIPrompt(ProductScanRequest("greenmarket.dot", ProductExecutionKind.APP, request), HostScan.Dismissed as HostScan)
        val scans = mockk<TrUAPIProductScans> { every { current } returns prompt }
        return prompt to TrUAPIProductScanViewModel(router, scans, mockk(), mockk())
    }

    @Test
    fun `a wrong code only shows the message and the matching one answers once`() = runTest {
        // The camera keeps reading after a match, and the product gets one answer.
        val (prompt, viewModel) = scan()
        val messages = mutableListOf<Unit>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) {
            viewModel.notForThisProduct.collect(messages::add)
        }

        viewModel.onCode(Barcode.FORMAT_QR_CODE, "https://elsewhere.example")
        viewModel.onCode(Barcode.FORMAT_QR_CODE, receipt)
        viewModel.onCode(Barcode.FORMAT_QR_CODE, receipt)

        assertEquals(1, messages.size)
        assertEquals(HostScan.Scanned(receipt, CodeFormat.QR), prompt.await())
        coVerify(exactly = 1) { router.closeTrUAPIProductScan() }
    }

    @Test
    fun `closing or a refused camera answers without a code`() = runTest {
        val (closed, closing) = scan()
        closing.onCloseClicked()
        assertEquals(HostScan.Dismissed, closed.await())

        val (refused, refusing) = scan()
        refusing.onPermissionAlertClosed()
        assertEquals(HostScan.CameraUnavailable, refused.await())
    }
}
