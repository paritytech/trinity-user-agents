import Foundation

public enum ChainSocketCloseReason: Sendable {
    case transport(Error?)
    case peerClosed
    case connectTimeout
    case livenessTimeout
    case resumed
}

public protocol ChainSocketDelegate: AnyObject {
    func chainSocketDidOpen(_ socket: ChainSocketProtocol)
    func chainSocket(_ socket: ChainSocketProtocol, didReceive text: String)
    func chainSocket(_ socket: ChainSocketProtocol, didCloseWith reason: ChainSocketCloseReason)
}

public extension ChainSocketDelegate {
    func chainSocketDidOpen(_: ChainSocketProtocol) {}
}

/// Single-use raw JSON-RPC socket to one node: never reconnects, reports a transport death once.
/// Delegate callbacks are delivered in order on the socket's serial queue.
public protocol ChainSocketProtocol: AnyObject, Sendable {
    var url: URL { get }
    var delegate: ChainSocketDelegate? { get set }

    func open()

    func send(text: String)

    /// Doesn't trigger `didCloseWith`.
    func close()

    /// Closes and reports `didCloseWith(.resumed)`; for sockets that may not have survived an app suspension.
    func expire()

    /// Reports `didCloseWith(.livenessTimeout)` unless a pong or any frame arrives within `timeout`.
    func probe(timeout: TimeInterval)
}
