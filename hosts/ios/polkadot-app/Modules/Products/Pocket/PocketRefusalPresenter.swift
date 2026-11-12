import UIKit
import UIKitExt

/// Tells the user a Pocket link went nowhere.
///
/// A link the host claimed but cannot serve must say so: staying silent reads
/// as the app having ignored the tap, and the user retries it.
@MainActor
final class PocketRefusalPresenter: AlertPresentable {
    static let shared = PocketRefusalPresenter()

    private init() {}

    static func show(_ message: String) {
        shared.present(
            message: message,
            title: nil,
            closeAction: String(localized: .Common.close),
            from: nil
        )
    }
}
