import SDKLogger

/// Core markers that may leave the device, as an allowlist rather than the
/// inverse: a marker the core adds later stays local until someone says what it
/// carries. `truapi.native.*` is what stays behind, because those lines carry
/// notification bodies, URLs and permission requests.
///
/// The Android host keeps the same list in `logCoreLine`.
private let exportedCoreMarkerPrefixes = [
    "truapi.tracing.",
    "truapi.chain.",
    "truapi.core.",
    "truapi.ws_bridge."
]

extension LoggerProtocol {
    /// Info lines go to Sentry; everything else stays on the device.
    func logCoreLine(marker: String, detail: String) {
        if exportedCoreMarkerPrefixes.contains(where: { prefix in marker.hasPrefix(prefix) }) {
            info("[truapi.core] \(marker): \(detail)")
        } else {
            debug("[truapi.core] \(marker): \(detail)")
        }
    }
}
