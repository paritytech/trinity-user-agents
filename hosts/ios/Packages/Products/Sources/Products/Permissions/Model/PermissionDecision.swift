import Foundation

/// Outcome of a permission prompt.
public enum PermissionDecision: Sendable, Equatable {
    case allowAlways
    case allowOnce
    case deny
}
