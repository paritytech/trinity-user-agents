import Foundation

/// The raw value is both the manifest `kind` discriminator and the `<kind>.<base>` subname label.
public enum ExecutableKind: String, CaseIterable, Sendable {
    case app
    case widget
    case worker
}
