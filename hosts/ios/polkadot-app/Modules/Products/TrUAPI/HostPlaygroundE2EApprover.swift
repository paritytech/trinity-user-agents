#if targetEnvironment(simulator) && HOST_PLAYGROUND_E2E
    import PolkadotUI
    import UIKit

    /// Answers the native confirmation sheets a host-playground test raises, such as the signing
    /// sheet and the product permission prompt, by sending the approving control its tap.
    ///
    /// Only controls inside presented view controllers, or in windows other than the tab bar's,
    /// are considered, so the product's own chrome is never tapped.
    @MainActor
    final class HostPlaygroundE2EApprover {
        /// Accessibility identifiers of approving controls, checked before any label.
        static let approvingIdentifiers = ["signing_approve_button"]

        /// Titles of approving controls, in order of preference when a sheet offers several.
        static let approvingLabels = ["Approve", "Allow once", "Allow always", "Sign", "Confirm"]

        private var task: Task<Void, Never>?
        private var lastSeen = ""

        /// Starts answering sheets until ``stop()``.
        func start() {
            stop()
            task = Task { [weak self] in
                while !Task.isCancelled {
                    let tapped = self?.tapApprovingControl() ?? false
                    // A tapped sheet takes a moment to dismiss; scanning it again would tap twice.
                    try? await Task.sleep(for: tapped ? .seconds(2) : .milliseconds(500))
                }
            }
        }

        /// Stops answering sheets.
        func stop() {
            task?.cancel()
            task = nil
        }

        private func tapApprovingControl() -> Bool {
            let controls = candidateRoots().flatMap { Self.controls(in: $0) }.filter(Self.isTappable)

            let byIdentifier = controls.first { control in
                control.accessibilityIdentifier.map(Self.approvingIdentifiers.contains) ?? false
            }
            let byLabel = Self.approvingLabels.lazy.compactMap { label in
                controls.first { Self.title(of: $0)?.caseInsensitiveCompare(label) == .orderedSame }
            }.first

            guard let control = byIdentifier ?? byLabel else {
                logSeen(controls)
                return false
            }

            let title = Self.title(of: control) ?? control.accessibilityIdentifier ?? "?"
            HostPlaygroundE2E.log.info("answering a sheet with \(title, privacy: .public)")
            control.sendActions(for: .touchUpInside)
            return true
        }

        /// Logs the controls on screen whenever they change, so a sheet it could not answer shows up
        /// in the run's log.
        private func logSeen(_ controls: [UIControl]) {
            let seen = controls
                .map { "\(type(of: $0)):\(Self.title(of: $0) ?? $0.accessibilityIdentifier ?? "-")" }
                .joined(separator: ", ")
            guard seen != lastSeen else {
                return
            }
            lastSeen = seen
            HostPlaygroundE2E.log.info("no approving control among: \(seen, privacy: .public)")
        }

        private func candidateRoots() -> [UIView] {
            let windows = UIApplication.shared.connectedScenes
                .compactMap { $0 as? UIWindowScene }
                .flatMap(\.windows)
                .filter { !$0.isHidden }

            return windows.flatMap { window -> [UIView] in
                var presented: [UIViewController] = []
                var next = window.rootViewController?.presentedViewController
                while let controller = next {
                    presented.append(controller)
                    next = controller.presentedViewController
                }

                let isTabBarWindow = window.rootViewController is MainTabBarViewController
                let root = isTabBarWindow ? [] : [window.rootViewController].compactMap { $0 }
                return (root + presented).compactMap(\.viewIfLoaded)
            }
        }

        private static func controls(in view: UIView) -> [UIControl] {
            let own = (view as? UIControl).map { [$0] } ?? []
            return own + view.subviews.flatMap { controls(in: $0) }
        }

        private static func isTappable(_ control: UIControl) -> Bool {
            guard control.isEnabled, control.isUserInteractionEnabled, control.window != nil else {
                return false
            }

            var view: UIView? = control
            while let current = view {
                if current.isHidden || current.alpha < 0.01 {
                    return false
                }
                view = current.superview
            }
            return true
        }

        private static func title(of control: UIControl) -> String? {
            // Design-system buttons render their title in SwiftUI, out of reach of the label walk.
            if let button = control as? DSButtonView {
                return button.title
            }
            let text = labels(in: control)
                .compactMap(\.text)
                .joined(separator: " ")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            return text.isEmpty ? control.accessibilityLabel : text
        }

        private static func labels(in view: UIView) -> [UILabel] {
            let own = (view as? UILabel).map { [$0] } ?? []
            return own + view.subviews.flatMap { labels(in: $0) }
        }
    }
#endif
