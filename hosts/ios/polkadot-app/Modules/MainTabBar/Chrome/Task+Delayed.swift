import Foundation

extension Task where Success == Void, Failure == Never {
    /// Runs `action` on the main actor after `seconds`, unless the task is cancelled first.
    static func delayed(
        by seconds: TimeInterval,
        _ action: @escaping @MainActor @Sendable () -> Void
    ) -> Task<Void, Never> {
        Task { @MainActor in
            try? await Task<Never, Never>.sleep(for: .seconds(seconds))
            guard !Task<Never, Never>.isCancelled else {
                return
            }
            action()
        }
    }
}
