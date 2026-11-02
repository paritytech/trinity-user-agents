package io.paritytech.polkadotapp.feature_products_impl.domain.spaBrowser.statusBar

import io.mockk.coEvery
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.presentation.sharing.SharingManager
import io.paritytech.polkadotapp.feature_products_api.domain.error.ProductResolutionError
import io.paritytech.polkadotapp.feature_products_api.model.Executables
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_api.model.SemVer
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

class RealProductStatusBarInteractorTest {
    private val productId = ProductId.fromStoredValue("dim2.paseo")

    private val resolveProductUseCase: ResolveProductUseCase = mockk()
    private val sharingManager: SharingManager = mockk(relaxUnitFun = true)

    private val interactor = RealProductStatusBarInteractor(
        integrationRepository = mockk(),
        chatRoomRepository = mockk(),
        resolveProductUseCase = resolveProductUseCase,
        sharingManager = sharingManager,
        dotNsTldProvider = mockk(),
    )

    @Test
    fun `chat is available when the worker serves it`() = runBlocking<Unit> {
        withWorker(worker(includesChat = true))

        assertEquals(Result.success(true), interactor.includesChat(productId))
    }

    @Test
    fun `chat is not available when the worker does not serve it`() = runBlocking<Unit> {
        withWorker(worker(includesChat = false))

        assertEquals(Result.success(false), interactor.includesChat(productId))
    }

    @Test
    fun `chat is not available without a worker`() = runBlocking<Unit> {
        withWorker(null)

        assertEquals(Result.success(false), interactor.includesChat(productId))
    }

    @Test
    fun `a failed resolution reaches the caller`() = runBlocking<Unit> {
        coEvery { resolveProductUseCase.resolve(productId) } returns Result.failure(ProductResolutionError.Unknown)

        assertEquals(Result.failure<Boolean>(ProductResolutionError.Unknown), interactor.includesChat(productId))
    }

    @Test
    fun `share sends the product's web mirror link`() {
        interactor.share(productId)
        interactor.share(ProductId.fromStoredValue("sub.browse.dot"))

        verify {
            sharingManager.shareText("https://dim2.paseo.li")
            sharingManager.shareText("https://sub.browse.dot.li")
        }
    }

    @Test
    fun `share sends the product's own link where no mirror serves it`() {
        interactor.share(ProductId.fromStoredValue("coinflip.westend"))

        verify { sharingManager.shareText("https://coinflip.westend") }
    }

    private fun withWorker(worker: ProductExecutable.Worker?) {
        coEvery { resolveProductUseCase.resolve(productId) } returns Result.success(
            ResolvedProduct(
                product = Product(id = productId, name = "Dim 2", icon = null),
                description = null,
                executables = Executables(app = null, widget = null, worker = worker),
            )
        )
    }

    private fun worker(includesChat: Boolean) = ProductExecutable.Worker(
        scriptUrl = "https://worker.dim2.paseo/worker.js",
        appVersion = SemVer.ZERO,
        includesChat = includesChat,
        includesPocket = false,
        pocketCards = emptyList(),
    )
}
