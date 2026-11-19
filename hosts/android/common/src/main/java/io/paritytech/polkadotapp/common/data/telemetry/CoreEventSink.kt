package io.paritytech.polkadotapp.common.data.telemetry

/**
 * Sends one core event to the reporting backend as a log whose attributes it can filter on.
 * `priority` is an `android.util.Log` level, and each attribute value is a `String`, `Long` or `Boolean`.
 */
fun interface CoreEventSink {
    fun log(priority: Int, marker: String, attributes: Map<String, Any>)
}
