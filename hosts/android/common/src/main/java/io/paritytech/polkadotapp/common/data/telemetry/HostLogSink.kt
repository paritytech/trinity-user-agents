package io.paritytech.polkadotapp.common.data.telemetry

/**
 * Sends one tagged host log line to the reporting backend, the same lines on Android and iOS.
 *
 * `priority` is an `android.util.Log` level, and each attribute value is a `String`, `Long` or `Boolean`.
 * Attributes arrive beside `tag` as fields the backend can filter on.
 */
fun interface HostLogSink {
    fun log(priority: Int, tag: String, message: String, attributes: Map<String, Any>)
}
