package io.paritytech.polkadotapp.feature_products_impl.domain.serialization

import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertEquals
import org.junit.Test

class JsonWidgetSerializerTest {
    // Products send image widgets in this shape, so an image source must keep its key and type names.
    @Test
    fun `an image widget names its source the way products send it`() {
        val decoded = listOf(
            """{"type":"image","source":{"type":"bulletin","cid":"bafy"}}""",
            """{"type":"image","source":{"type":"archive","path":"faces/stamp.png"}}""",
        ).map { JsonWidgetSerializer().deserialize(it).getOrThrow() }

        assertEquals(
            listOf(JsWidget.Image(source = JsImageSource.Bulletin("bafy")), JsWidget.Image(source = JsImageSource.Archive("faces/stamp.png"))),
            decoded,
        )
    }
}
