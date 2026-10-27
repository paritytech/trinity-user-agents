import Foundation
import enum TrUAPIHost.ProductExecutionKind

/// The raw value is both the manifest `kind` discriminator and the `<kind>.<base>` subname label.
public enum ExecutableKind: String, CaseIterable, Sendable {
    case app
    case widget
    case worker
}

extension ExecutableKind {
    var executionKind: ProductExecutionKind {
        switch self {
        case .app: .app
        case .widget: .widget
        case .worker: .worker
        }
    }
}
