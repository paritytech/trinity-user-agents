import Products
import TrUAPIHost

extension ExecutableKind {
    /// The execution the core opens for this executable, which decides what
    /// its page may call.
    var executionKind: ProductExecutionKind {
        switch self {
        case .app: .app
        case .widget: .widget
        case .worker: .worker
        }
    }
}
