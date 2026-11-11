package io.paritytech.polkadotapp.feature_products_impl.data.storage

import com.google.gson.Gson
import io.mockk.every
import io.mockk.mockk
import io.mockk.slot
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductRoomFooter
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ProductId

class ProductRoomFooterStorageTest {
    private val product = ProductId.parse("dim2.dot").getOrThrow()

    /** A restart reads back exactly what the product docked, payload bytes included. */
    @Test
    fun `footers read back as they were written`() {
        val stored = slot<String>()
        val preferences = mockk<Preferences> {
            every { putString(any(), capture(stored)) } returns Unit
            every { getString(any<String>()) } answers { stored.captured }
        }
        val storage = RealProductRoomFooterStorage(preferences, Gson())
        val footers = mapOf(
            ProductChatIdParameter("jollity") to ProductRoomFooter.Card("register", byteArrayOf(1, 2, -1)),
            ProductChatIdParameter("lobby") to ProductRoomFooter.Empty,
        )

        storage.write(product, footers)

        assertEquals(footers, storage.read(product))
    }

    @Test
    fun `a product that never set a footer reads no footers`() {
        val preferences = mockk<Preferences> { every { getString(any<String>()) } returns null }

        assertEquals(emptyMap<ProductChatIdParameter, ProductRoomFooter>(), RealProductRoomFooterStorage(preferences, Gson()).read(product))
    }
}
