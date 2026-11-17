package io.paritytech.polkadotapp.feature_products_impl.domain.spaBrowser

import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.presentation.sharing.SharingManager
import org.junit.Test
import uniffi.truapi.ProductId

class RealSpaBrowserInteractorTest {
    private val productId = ProductId.parse("dim2.paseo").getOrThrow()

    private val sharingManager: SharingManager = mockk(relaxUnitFun = true)

    private val interactor = RealSpaBrowserInteractor(
        resolveProductUseCase = mockk(),
        integrationRepository = mockk(),
        chatRoomRepository = mockk(),
        sharingManager = sharingManager,
        dotNsTldProvider = mockk(),
    )

    @Test
    fun `share sends the product's web mirror link`() {
        interactor.share(productId)
        interactor.share(ProductId.parse("sub.browse.dot").getOrThrow())

        verify {
            sharingManager.shareText("https://dim2.paseo.li")
            sharingManager.shareText("https://sub.browse.dot.li")
        }
    }

    @Test
    fun `share sends a development product's own link`() {
        interactor.share(ProductId.parse("localhost:3000").getOrThrow())

        verify { sharingManager.shareText("https://localhost:3000") }
    }
}
