package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import android.content.Context
import android.net.Uri
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.message.ProductsMessageRenderer
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductWorkerArchive
import io.paritytech.polkadotapp.feature_products_impl.domain.product.RealProductImageResolver
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test
import java.io.File

/**
 * A chat card names its images the way a Pocket face does, so the extension the factory builds
 * hands its card renderer the same places to read them from: the product's worker archive, which
 * serves the chat, and the Bulletin gateway.
 */
class ProductChatCardImagesTest {
    private val product = Product(ProductId.fromStoredValue("game.paseo"), "Game", icon = null)
    private val archive: ProductWorkerArchive = mockk()
    private val ipfsContentLookup: IpfsContentLookup = mockk()
    private val picture: Uri = mockk()

    @Before
    fun stubUris() = mockkStatic(Uri::class)

    @After
    fun restoreUris() = unmockkStatic(Uri::class)

    @Test
    fun `reads an archive image out of the product's worker archive`() = runTest {
        val file = File("/archive/art/prize.png")
        coEvery { archive.file(product.id, "art/prize.png") } returns Result.success(file)
        every { Uri.fromFile(file) } returns picture

        assertEquals(picture, chatImages().resolve(JsImageSource.Archive("art/prize.png")))
    }

    @Test
    fun `reads a bulletin image through the gateway`() = runTest {
        coEvery { ipfsContentLookup.getIpfsLinkFor("bafyprize") } returns Result.success("https://gateway.invalid/ipfs/bafyprize")
        every { Uri.parse("https://gateway.invalid/ipfs/bafyprize") } returns picture

        assertEquals(picture, chatImages().resolve(JsImageSource.Bulletin("bafyprize")))
        coVerify(exactly = 0) { archive.file(any(), any()) }
    }

    // The renderer draws the unavailable icon for nothing, so a broken source costs the card one node.
    @Test
    fun `an image missing from the archive resolves to nothing`() = runTest {
        coEvery { archive.file(product.id, "art/missing.png") } returns Result.failure(IllegalArgumentException("not in the archive"))

        assertNull(chatImages().resolve(JsImageSource.Archive("art/missing.png")))
    }

    private fun chatImages(): JsImageResolver {
        val factory = ProductBotFactory(
            appContext = mockk<Context>(relaxed = true),
            workerRefCounter = mockk(),
            imageResolver = RealProductImageResolver(archive, ipfsContentLookup),
        )
        val extension = factory.create(product, ProductChatExtensionOrigin.InstalledByUser)
        val renderer = extension.customMessageRenderers().single() as ProductsMessageRenderer

        return renderer.imageResolver
    }
}
