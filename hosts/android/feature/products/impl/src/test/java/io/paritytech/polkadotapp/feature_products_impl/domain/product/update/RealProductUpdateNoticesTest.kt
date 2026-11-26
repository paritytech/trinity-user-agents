package io.paritytech.polkadotapp.feature_products_impl.domain.product.update

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.domain.product.PendingProductUpdate
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ProductId

class RealProductUpdateNoticesTest {
    private val tld = requireNotNull(DotNsTld.parse("dot"))
    private val personhood = ReservedProductIds.personhood(tld)
    private val kart = ProductId.parse("kart.dot").getOrThrow()

    private val foundUpdates = MutableSharedFlow<Set<String>>()

    private val productUpdater: ProductUpdater = mockk {
        every { observePendingNames() } returns MutableStateFlow(setOf(PERSONHOOD_APP, KART_APP))
        every { observeFoundUpdates() } returns foundUpdates
    }
    private val resolveProductUseCase: ResolveProductUseCase = mockk {
        coEvery { resolve(any(), any()) } returns Result.failure(IllegalStateException("manifest not readable"))
    }
    private val dotNsTldProvider: DotNsTldProvider = mockk {
        coEvery { getTld() } returns Result.success(tld)
    }
    private val productsInUse = ProductsInUse()

    private val notices = RealProductUpdateNotices(productUpdater, productsInUse, resolveProductUseCase, dotNsTldProvider)

    @Test
    fun `a page of a built-in product gets no notice while a page of another product does`() = runTest {
        showPage(personhood, PERSONHOOD_APP)
        showPage(kart, KART_APP)

        assertEquals(KART_APP, notices.observeNotice().first()?.dotNsName)
    }

    @Test
    fun `a dismissed notice comes back when a check finds the update again`() = runTest {
        showPage(kart, KART_APP)
        notices.start(backgroundScope)
        val shownNotices = mutableListOf<PendingProductUpdate?>()
        backgroundScope.launch { notices.observeNotice().toList(shownNotices) }
        runCurrent()

        notices.dismiss(requireNotNull(shownNotices.last()))
        runCurrent()
        foundUpdates.emit(setOf(KART_APP))
        runCurrent()

        assertEquals(listOf(KART_APP, null, KART_APP), shownNotices.map { it?.dotNsName })
    }

    private fun TestScope.showPage(productId: ProductId, dotNsName: String) {
        productsInUse.trackPage(MutableStateFlow(ShownPage(productId, dotNsName)), backgroundScope, reload = {})
        runCurrent()
    }

    private companion object {
        const val PERSONHOOD_APP = "app.peopl.dot"
        const val KART_APP = "app.kart.dot"
    }
}
