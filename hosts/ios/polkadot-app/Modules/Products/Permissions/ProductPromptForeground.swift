import Products
import UIKit

/// Waits until the app is in the foreground, so that a prompt can get an answer from the user.
///
/// An inactive app is in the foreground: it is inactive while a system alert or a transition shows, and it can
/// still present a sheet.
struct ApplicationForegroundWaiter: ProductPromptForegroundWaiting {
    func waitForForeground(timeout: Duration) async -> Bool {
        if await Self.isInForeground() {
            return true
        }

        let cameToForeground = await withTaskGroup(of: Bool.self) { group in
            group.addTask {
                let names = [UIApplication.willEnterForegroundNotification, UIApplication.didBecomeActiveNotification]
                return await withTaskGroup(of: Bool.self) { inner in
                    for name in names {
                        inner.addTask {
                            for await _ in NotificationCenter.default.notifications(named: name) {
                                return true
                            }
                            return false
                        }
                    }
                    let first = await inner.next() ?? false
                    inner.cancelAll()
                    return first
                }
            }
            group.addTask {
                try? await Task.sleep(for: timeout)
                return false
            }
            let first = await group.next() ?? false
            group.cancelAll()
            return first
        }

        // The app can come to the foreground before the observation starts.
        return cameToForeground ? true : await Self.isInForeground()
    }

    @MainActor
    private static func isInForeground() -> Bool {
        UIApplication.shared.applicationState != .background
    }
}
