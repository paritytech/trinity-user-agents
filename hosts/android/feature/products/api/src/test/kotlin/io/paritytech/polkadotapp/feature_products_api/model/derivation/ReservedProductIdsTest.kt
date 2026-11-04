package io.paritytech.polkadotapp.feature_products_api.model.derivation

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ProductId

class ReservedProductIdsTest {
    // Humanity runs the game in its Polkadot room (individuality-apps#1281), but the game is
    // played in the game product of the same network.
    @Test
    fun `Humanity's game reminder opens the game product of its own network`() {
        for ((bookedBy, opens) in listOf("peopl.dot" to "dim2.dot", "peopl.paseo" to "dim2.paseo")) {
            assertEquals(bookedBy, id(opens), ReservedProductIds.gameReminderOpenTarget(id(bookedBy)))
        }
    }

    // The game product's own reminder, and any lookalike of Humanity, open the booking product.
    @Test
    fun `any other product's game reminder opens that product`() {
        for (bookedBy in listOf("dim2.dot", "dim2.paseo", "app.peopl.paseo", "peoplx.dot", "game.dot")) {
            assertEquals(bookedBy, id(bookedBy), ReservedProductIds.gameReminderOpenTarget(id(bookedBy)))
        }
    }

    private fun id(value: String) = ProductId.parse(value).getOrThrow()
}
