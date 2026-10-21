package io.paritytech.polkadotapp.feature_products_api.model.derivation

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import org.junit.Assert.assertEquals
import org.junit.Test

class ReservedProductIdsTest {
    // The chat product gets the core runtime and its PersonChat whatever the runtime setting says, so a
    // product under another name, a subname included, must not be taken for it.
    @Test
    fun `only chat dot tld is the chat product`() {
        val ids = listOf("chat.dot", "chat.paseo", "app.chat.dot", "chatx.dot", "chat")
        assertEquals(
            listOf(true, true, false, false, false),
            ids.map { ReservedProductIds.isChat(ProductId.fromStoredValue(it)) },
        )
    }
}
