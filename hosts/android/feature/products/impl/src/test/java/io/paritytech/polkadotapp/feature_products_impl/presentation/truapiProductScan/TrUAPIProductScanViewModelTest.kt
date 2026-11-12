package io.paritytech.polkadotapp.feature_products_impl.presentation.truapiProductScan

import com.google.mlkit.vision.barcode.common.Barcode
import io.mockk.Called
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.presentation.camera.CameraQrReader
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.utils.permissions.PermissionAsker
import io.paritytech.polkadotapp.common.utils.permissions.PermissionResult
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductScanRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIProductScans
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIPrompt
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.CompletableDeferred
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
import uniffi.truapi.ProductId
import uniffi.truapi.CodeFormat
import uniffi.truapi.HostScan
import uniffi.truapi.HostScannerScanRequest
import uniffi.truapi.ExecutableKind

@OptIn(ExperimentalCoroutinesApi::class)
class TrUAPIProductScanViewModelTest {
    private val router = mockk<ProductsRouter>(relaxed = true)
    private val receipt = "https://greenmarket.example/r/BAG6"

    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun scan(
        permissions: PermissionAsker = mockk(),
        camera: CameraQrReader = mockk(),
    ): Pair<TrUAPIPrompt<ProductScanRequest, HostScan>, TrUAPIProductScanViewModel> {
        val request = HostScannerScanRequest(listOf(CodeFormat.QR), "https://greenmarket.example/r/", null)
        val prompt = TrUAPIPrompt(ProductScanRequest(ProductId.parse("greenmarket.dot").getOrThrow(), ExecutableKind.APP, request), HostScan.Dismissed as HostScan)
        val scans = mockk<TrUAPIProductScans> { every { current } returns prompt }
        return prompt to TrUAPIProductScanViewModel(router, scans, permissions, camera)
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
        assertEquals(
            LoadingState.Loaded(TrUAPIProductScanUiState(closed.question.productId, closed.question.request.hint)),
            closing.state.value,
        )
        closing.onCloseClicked()
        assertEquals(LoadingState.Loading, closing.state.value)
        assertEquals(HostScan.Dismissed, closed.await())

        val (refused, refusing) = scan()
        refusing.onPermissionAlertClosed()
        assertEquals(HostScan.CameraUnavailable, refused.await())
    }

    @Test
    fun `a restored scanner without a live prompt closes without requesting the camera`() = runTest {
        val request = HostScannerScanRequest(listOf(CodeFormat.QR), null, null)
        val answered = TrUAPIPrompt(
            ProductScanRequest(ProductId.parse("greenmarket.dot").getOrThrow(), ExecutableKind.APP, request),
            HostScan.Dismissed as HostScan,
        )
        answered.dismiss()
        val permissions = mockk<PermissionAsker>()
        val camera = mockk<CameraQrReader>()

        for (prompt in listOf(null, answered)) {
            val scans = mockk<TrUAPIProductScans> { every { current } returns prompt }
            val viewModel = TrUAPIProductScanViewModel(router, scans, permissions, camera)

            viewModel.onShown()
            viewModel.bindToCamera(mockk())

            assertEquals(LoadingState.Loading, viewModel.state.value)
        }
        coVerify(exactly = 2) { router.closeTrUAPIProductScan() }
        verify {
            permissions wasNot Called
            camera wasNot Called
        }
    }

    @Test
    fun `permission granted after closing does not bind the camera`() = runTest {
        val permission = CompletableDeferred<PermissionResult>()
        val permissions = mockk<PermissionAsker> {
            coEvery { askPermission(any()) } coAnswers { permission.await() }
        }
        val camera = mockk<CameraQrReader>(relaxed = true)
        val (prompt, viewModel) = scan(permissions, camera)
        val binding = launch(UnconfinedTestDispatcher(testScheduler)) { viewModel.bindToCamera(mockk()) }
        coVerify(exactly = 1) { permissions.askPermission(any()) }

        viewModel.onCloseClicked()
        permission.complete(PermissionResult.GRANTED)
        binding.join()

        verify { camera wasNot Called }
        assertEquals(HostScan.Dismissed, prompt.await())
    }
}
