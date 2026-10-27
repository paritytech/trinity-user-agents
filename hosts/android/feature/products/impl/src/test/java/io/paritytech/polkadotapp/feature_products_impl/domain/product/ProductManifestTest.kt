package io.paritytech.polkadotapp.feature_products_impl.domain.product

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableHost
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableKind
import io.paritytech.polkadotapp.feature_products_api.model.productIdFromHost
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ProductId

class ProductManifestTest {
    private val tld = DotNsTld.parse("dot")!!
    private val productId = ProductId.parse("coinflip.dot")

    @Test
    fun `each kind is served from its own labelled subname`() {
        assertEquals(ExecutableHost("app.coinflip.dot"), ProductManifest.hostOf(productId, ExecutableKind.APP))
        assertEquals(ExecutableHost("widget.coinflip.dot"), ProductManifest.hostOf(productId, ExecutableKind.WIDGET))
        assertEquals(ExecutableHost("worker.coinflip.dot"), ProductManifest.hostOf(productId, ExecutableKind.WORKER))
    }

    @Test
    fun `the label a subname carries is the one ProductId drops`() {
        val host = ProductManifest.hostOf(productId, ExecutableKind.APP)

        assertEquals(productId, productIdFromHost(host.value, tld).getOrThrow())
    }
}
