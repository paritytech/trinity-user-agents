import Foundation

public enum ChainSocketCloseReason: Sendable {
    case transport(Error?)
    case peerClosed
    case livenessTimeout
    case sleep
}

public protocol ChainSocketDelegate: AnyObject {
    func chainSocket(_ socket: ChainSocketProtocol, didReceive text: String)
    func chainSocket(_ socket: ChainSocketProtocol, didCloseWith reason: ChainSocketCloseReason)
}

/// Single-use raw JSON-RPC socket to one node: never reconnects, reports a transport death once.
public protocol ChainSocketProtocol: AnyObject, Sendable {
    var delegate: ChainSocketDelegate? { get set }

    func open()

    func send(text: String)

    /// Doesn't trigger `didCloseWith`.
    func close()
}
