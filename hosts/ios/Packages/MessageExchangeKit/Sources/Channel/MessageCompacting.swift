import Foundation

public protocol MessageCompacting: Sendable {
    associatedtype Message: MessageExchange.CodableMessage

    func compact(messages: [Message]) async throws -> Message
}

// MARK: - Type Erasure Implementation

public protocol MessageCompactorMaking: Sendable {
    associatedtype Message: MessageExchange.CodableMessage

    func createCompactor() throws -> AnyMessageCompactor<Message>?
}

public final class AnyMessageCompactorFactory<M: MessageExchange.CodableMessage>: MessageCompactorMaking,
    @unchecked Sendable {
    public typealias Message = M

    private let createClosure: @Sendable () throws -> AnyMessageCompactor<M>?

    public init<Factory: MessageCompactorMaking>(_ factory: Factory) where Factory.Message == M {
        createClosure = {
            try factory.createCompactor()
        }
    }

    public func createCompactor() throws -> AnyMessageCompactor<M>? {
        try createClosure()
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
