package io.paritytech.polkadotapp.feature_products_api.model

import com.google.gson.Gson
import org.junit.Assert.assertEquals
import org.junit.Test

/** The JS host API carries these names verbatim; the core's own spelling is ROOM_HOST/BOT. */
class RoomParticipationTest {
    private val gson = Gson()

    @Test
    fun `the wire spelling is the core vocabulary, not the enum name`() {
        assertEquals("\"RoomHost\"", gson.toJson(RoomParticipation.ROOM_HOST))
        assertEquals("\"Bot\"", gson.toJson(RoomParticipation.BOT))
    }
}
