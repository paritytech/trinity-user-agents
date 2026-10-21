import Foundation

/// No screen got an answer from the user for a permission prompt.
///
/// This is not a refusal. A caller must not keep it as a decision, so the next request asks again. The core gets
/// it as an error, and an error leaves the permission "not determined".
public enum ProductPermissionPromptError: Error, Equatable, Sendable {
    /// The app did not come to the foreground in the time that the requester waits.
    case notInForeground
    /// No screen can show the prompt: no view is attached, or the presentation did not occur.
    case cannotShow
    /// The prompt went off the screen before the user answered it.
    case closedWithoutAnswer
}

/// Tells when the app is in the foreground, so that a prompt can get an answer.
public protocol ProductPromptForegroundWaiting: Sendable {
    /// Returns true when the app is in the foreground, or comes to the foreground before `timeout` ends.
    func waitForForeground(timeout: Duration) async -> Bool
}

/// A foreground that is always there. Tests and hosts with no app lifecycle use it.
public struct AlwaysInForeground: ProductPromptForegroundWaiting {
    public init() {}

    public func waitForForeground(timeout _: Duration) async -> Bool {
        true
    }
}
