package io.paritytech.polkadotapp.feature_products_impl.domain.bot.external

import android.content.Context
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import io.paritytech.polkadotapp.common.R as RCommon

class HostPlacedProductsTest {
    private val tld = requireNotNull(DotNsTld.parse("paseo"))
    private val context: Context = mockk {
        every { getString(RCommon.string.host_placed_chat_polkadot) } returns "Polkadot"
        every { getString(RCommon.string.host_placed_chat_jollity) } returns "Jollity"
    }
    private val tldProvider: DotNsTldProvider = mockk()

    private val hostPlacedProducts = HostPlacedProducts(context, tldProvider)

    private val polkadot = HostPlacedProduct(
        Product(ReservedProductIds.personhood(tld), "Polkadot", icon = null),
        ProductChatIdParameter("polkadot"),
    )
    private val jollity = HostPlacedProduct(
        Product(ReservedProductIds.game(tld), "Jollity", icon = null),
        ProductChatIdParameter("jollity"),
    )

    // The first stage of individuality-apps#1281: Humanity's `polkadot` room is placed beside the
    // game's `jollity`, which stays placed so the game's worker can hand over before a later
    // release stops placing it. The room ids are the contract with each product's worker. A TLD
    // persisted by a previous process costs no chain read, so both start with the first list.
    @Test
    fun `the host places Humanity's Polkadot room beside Jollity, at once when the TLD is already known`() = runTest {
        every { tldProvider.currentTldOrNull() } returns tld

        assertEquals(listOf(listOf(polkadot, jollity)), hostPlacedProducts.observe().toList())
    }

    // Every other product's extension is combined with this list, so a device that has never read
    // the TLD must not keep the chats it already has off the screen.
    @Test
    fun `nothing is placed until the TLD is read, so other products do not wait`() = runTest {
        every { tldProvider.currentTldOrNull() } returns null
        coEvery { tldProvider.getTld() } returns Result.success(tld)

        assertEquals(listOf(emptyList(), listOf(polkadot, jollity)), hostPlacedProducts.observe().toList())
    }
}
