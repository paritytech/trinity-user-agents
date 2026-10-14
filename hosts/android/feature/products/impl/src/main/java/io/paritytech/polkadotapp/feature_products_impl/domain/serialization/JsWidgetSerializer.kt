package io.paritytech.polkadotapp.feature_products_impl.domain.serialization

import io.paritytech.polkadotapp.feature_products_api.model.JsWidget

/**
 * Abstraction for widget deserialization.
 */
interface JsWidgetSerializer {
    /**
     * Deserialize a hex string (SCALE-encoded) to a widget.
     */
    fun deserialize(data: String): Result<JsWidget>

    /**
     * Deserialize raw bytes (SCALE-encoded) to a widget.
     */
    fun deserializeFromBytes(data: ByteArray): Result<JsWidget>
}
