package io.paritytech.polkadotapp.feature_products_impl.domain.serialization

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_impl.domain.serialization.scale.ScaleWidget
import io.paritytech.polkadotapp.feature_products_impl.domain.serialization.scale.toJsWidget
import kotlinx.serialization.decodeFromByteArray
import javax.inject.Inject

/**
 * Widget serializer that decodes SCALE-encoded bytes from the host-api protocol.
 *
 * [deserialize] accepts a hex string (with or without "0x" prefix) — the format
 * used by the container's `callNative('chatRenderWidget', { scaleHex })`.
 *
 * [deserializeFromBytes] accepts raw SCALE bytes directly.
 */
class ScaleWidgetSerializer @Inject constructor() : JsWidgetSerializer {
    override fun deserialize(data: String): Result<JsWidget> = runCatching {
        val bytes = data.fromHex()
        val scaleWidget = BinaryScale.decodeFromByteArray<ScaleWidget>(bytes)
        scaleWidget.toJsWidget()
    }

    override fun deserializeFromBytes(data: ByteArray): Result<JsWidget> = runCatching {
        val scaleWidget = BinaryScale.decodeFromByteArray<ScaleWidget>(data)
        scaleWidget.toJsWidget()
    }
}
