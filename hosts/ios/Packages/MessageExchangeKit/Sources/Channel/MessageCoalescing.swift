import Foundation

public enum MessageCoalescingDecision: Equatable {
    case append
    case replace
    case ignore
}

public protocol MessageCoalescing: Sendable {
    associatedtype Message: MessageExchange.CodableMessage

    func canCoalesce(_ message: Message) -> Bool
    func decision(for newMessage: Message, against pendingMessage: Message) -> MessageCoalescingDecision
}

// MARK: - Type Erasure Implementation

public final class AnyMessageCoalescer<M: MessageExchange.CodableMessage>: MessageCoalescing, @unchecked Sendable {
    public typealias Message = M

    public static var none: AnyMessageCoalescer<M> {
        AnyMessageCoalescer(
            canCoalesceClosure: { _ in false },
            decisionClosure: { _, _ in .append }
        )
    }

    private let canCoalesceClosure: (M) -> Bool
    private let decisionClosure: (M, M) -> MessageCoalescingDecision

    public init<Coalescer: MessageCoalescing>(_ coalescer: Coalescer) where Coalescer.Message == M {
        canCoalesceClosure = { message in
            coalescer.canCoalesce(message)
        }

        decisionClosure = { newMessage, pendingMessage in
            coalescer.decision(for: newMessage, against: pendingMessage)
        }
    }

    private init(
        canCoalesceClosure: @escaping (M) -> Bool,
        decisionClosure: @escaping (M, M) -> MessageCoalescingDecision
    ) {
        self.canCoalesceClosure = canCoalesceClosure
        self.decisionClosure = decisionClosure
    }

    public func canCoalesce(_ message: M) -> Bool {
        canCoalesceClosure(message)
    }

    public func decision(for newMessage: M, against pendingMessage: M) -> MessageCoalescingDecision {
        decisionClosure(newMessage, pendingMessage)
    }
}

enum MessageCoalescingOutcome<M> {
    case append
    case ignore
    case replace(M)
}

extension AnyMessageCoalescer {
    func outcome(for message: M, pendingMessages: [M]) -> MessageCoalescingOutcome<M> {
        guard canCoalesce(message) else {
            return .append
        }

        var replacedMessage: M?

        for pendingMessage in pendingMessages where canCoalesce(pendingMessage) {
            switch decision(for: message, against: pendingMessage) {
            case .ignore:
                return .ignore
            case .replace:
                replacedMessage = replacedMessage ?? pendingMessage
            case .append:
                continue
            }
        }

        return replacedMessage.map { .replace($0) } ?? .append
    }
}
