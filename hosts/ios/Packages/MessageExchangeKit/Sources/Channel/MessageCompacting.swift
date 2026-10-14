import Foundation

public protocol MessageCompacting: Sendable {
    associatedtype Message: MessageExchange.CodableMessage

    func compact(messages: [Message]) async throws -> Message
}

// MARK: - Type Erasure Implementation

public protocol MessageCompactorMaking: Sendable {
    associatedtype Message: MessageExchange.CodableMessage

    func createCompactor(for signKeyId: String) -> AnyMessageCompactor<Message>?
}

public final class AnyMessageCompactorFactory<M: MessageExchange.CodableMessage>: MessageCompactorMaking,
    @unchecked Sendable {
    public typealias Message = M

    private let createClosure: @Sendable (String) -> AnyMessageCompactor<M>?

    public init<Factory: MessageCompactorMaking>(_ factory: Factory) where Factory.Message == M {
        createClosure = { signKeyId in
            factory.createCompactor(for: signKeyId)
        }
    }

    public func createCompactor(for signKeyId: String) -> AnyMessageCompactor<M>? {
        createClosure(signKeyId)
    }
}

// MARK: - Type Erasure Implementation

public final class AnyMessageCompactor<M: MessageExchange.CodableMessage>: MessageCompacting, @unchecked Sendable {
    public typealias Message = M

    private let compactClosure: ([M]) async throws -> M

    public init<Compactor: MessageCompacting>(_ compactor: Compactor) where Compactor.Message == M {
        compactClosure = { messages in
            try await compactor.compact(messages: messages)
        }
    }

    public func compact(messages: [M]) async throws -> M {
        try await compactClosure(messages)
    }
}
