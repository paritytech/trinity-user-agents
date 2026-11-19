package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.data.telemetry.CoreEventSink
import timber.log.Timber
import uniffi.truapi.CoreAttribute
import uniffi.truapi.CoreAttributeValue
import javax.inject.Inject

/**
 * Logs the core events both host bridges receive. Each event stays on the device as a debug line, and a
 * marker the core's table exports also goes to [CoreEventSink] at the priority the table carries. The
 * debug line stays below what `SentryLogTree` forwards, so an exported event reaches Sentry once, with
 * its attributes.
 */
class CoreEventLogger @Inject constructor(
    private val sink: CoreEventSink,
) {
    fun log(marker: String, attributes: List<CoreAttribute>) {
        val fields = attributes.joinToString(" ") { "${it.key}=${it.value.reported()}" }
        Timber.tag("truapi.core").d("%s: %s", marker, fields)
        val priority = CORE_MARKER_PRIORITIES[marker] ?: return
        sink.log(priority, marker, attributes.associate { it.key to it.value.reported() })
    }

    private fun CoreAttributeValue.reported(): Any = when (this) {
        is CoreAttributeValue.Text -> value
        is CoreAttributeValue.Count -> minOf(value, Long.MAX_VALUE.toULong()).toLong()
        is CoreAttributeValue.Flag -> value
    }
}
