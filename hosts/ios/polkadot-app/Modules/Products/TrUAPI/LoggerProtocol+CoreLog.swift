import SDKLogger

extension LoggerProtocol {
    /// Info lines go to Sentry, and the other core markers carry notification bodies, URLs and permission
    /// requests, so only the core tracing warnings and errors are logged at info.
    func logCoreLine(marker: String, detail: String) {
        if marker.hasPrefix("truapi.tracing.") {
            info("[truapi.core] \(marker): \(detail)")
        } else {
            debug("[truapi.core] \(marker): \(detail)")
        }
    }
}
