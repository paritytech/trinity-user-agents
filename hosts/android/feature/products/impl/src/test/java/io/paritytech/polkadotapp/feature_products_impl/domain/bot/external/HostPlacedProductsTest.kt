package io.paritytech.polkadotapp.feature_products_impl.domain.bot.external

import android.content.Context
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

class HostPlacedProductsTest {
    private val tld = requireNotNull(DotNsTld.parse("paseo"))
    private val context: Context = mockk {
        every { getString(any()) } returns "Jollity"
    }
    private val tldProvider: DotNsTldProvider = mockk()

    private val hostPlacedProducts = HostPlacedProducts(context, tldProvider)

    private val jollity = Product(ReservedProductIds.game(tld), "Jollity", icon = null)

    // The Jollity leg of #562: a build has to carry the game's chat with no add, install or
    // discover step. A TLD persisted by a previous process costs no chain read, so the game's
    // extension starts with the first list rather than a few seconds later.
    @Test
    fun `the host places Jollity alone, at once when the TLD is already known`() = runTest {
        every { tldProvider.currentTldOrNull() } returns tld

        assertEquals(listOf(listOf(jollity)), hostPlacedProducts.observe().toList())
    }

    // Every other product's extension is combined with this list, so a device that has never read
    // the TLD must not keep the chats it already has off the screen.
    @Test
    fun `nothing is placed until the TLD is read, so other products do not wait`() = runTest {
        every { tldProvider.currentTldOrNull() } returns null
        coEvery { tldProvider.getTld() } returns Result.success(tld)

        assertEquals(listOf(emptyList(), listOf(jollity)), hostPlacedProducts.observe().toList())
    }
}
