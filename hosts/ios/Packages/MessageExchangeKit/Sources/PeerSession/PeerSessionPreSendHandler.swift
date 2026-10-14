import Foundation

public protocol PeerSessionPreSendHandling {
    associatedtype Message

    func handlePreSend(message: Message)
}

// MARK: - Type Erasure Implementation

public final class AnyPeerSessionPreSendHandler<M>: PeerSessionPreSendHandling {
    public typealias Message = M

    private let handlePreSendClosure: (M) -> Void

    init<Handler: PeerSessionPreSendHandling>(_ targetHandler: Handler) where Handler.Message == M {
        handlePreSendClosure = { message in
            targetHandler.handlePreSend(message: message)
        }
    }

    public init(closure: @escaping (M) -> Void) {
        handlePreSendClosure = closure
    }

    public func handlePreSend(message: Message) {
        handlePreSendClosure(message)
    }
}

public extension AnyPeerSessionPreSendHandler {
    static func empty() -> AnyPeerSessionPreSendHandler<M> {
        .init(closure: { _ in })
    }
}
