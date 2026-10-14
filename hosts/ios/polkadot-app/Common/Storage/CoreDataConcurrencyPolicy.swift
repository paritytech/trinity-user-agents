import Foundation
import Operation_iOS

/// The Core Data topology a target runs with.
///
/// Both modes are named here and the choice is made at compile time from the target being built: the
/// extension declares `NOTIFICATION_SERVICE_EXTENSION` in `NotificationServiceExtension/Configs/*.xcconfig`,
/// the app declares nothing and gets the split. A target that links these files therefore states its own
/// mode, rather than the store inferring one from the bundle it happens to be running in.
enum CoreDataConcurrencyPolicy {
    /// The app: one writer, one observer carrying every subscription, and short-lived readers beside them.
    static let app: CoreDataConcurrencyMode = .concurrent(readerConcurrency: 2)

    /// The extension: one context for every role, as in 2.x. It has a 24 MB memory ceiling, no
    /// subscriptions and a single writer, so the split would only add contexts it never reads from.
    static let notificationServiceExtension: CoreDataConcurrencyMode = .serial

    /// The mode of the target this file was compiled into.
    static var forCurrentTarget: CoreDataConcurrencyMode {
        #if NOTIFICATION_SERVICE_EXTENSION
            notificationServiceExtension
        #else
            app
        #endif
    }
}
