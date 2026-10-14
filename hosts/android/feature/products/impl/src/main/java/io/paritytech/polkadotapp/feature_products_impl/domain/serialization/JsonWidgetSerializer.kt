package io.paritytech.polkadotapp.feature_products_impl.domain.serialization

import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import kotlinx.serialization.json.Json
import javax.inject.Inject

/**
 * JSON-based implementation of JsWidgetSerializer.
 * Used for communication between JS and native.
 */
class JsonWidgetSerializer @Inject constructor() : JsWidgetSerializer {
    private val json = Json {
        ignoreUnknownKeys = true
        isLenient = true
        encodeDefaults = true
        classDiscriminator = "type"
    }

    override fun deserialize(data: String): Result<JsWidget> {
        return runCatching {
            json.decodeFromString(JsWidget.serializer(), data)
        }
    }

    override fun deserializeFromBytes(data: ByteArray): Result<JsWidget> {
        return deserialize(data.toString(Charsets.UTF_8))
    }
}
