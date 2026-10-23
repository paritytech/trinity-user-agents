import AsyncExtensions
import Foundation

public enum ProductLoadPolicy: Equatable, Sendable {
    case untilCancelled
    case attempts(Int)
}

public protocol ProductContentLoading: Sendable {
    func loadAppContent(of productId: ProductId, policy: ProductLoadPolicy) async throws -> URL
}

public protocol ProductLoadBackgroundExecuting: Sendable {
    func execute<T: Sendable>(_ operation: @escaping @Sendable () async throws -> T) async throws -> T
}

public struct ProductLoadBackgroundExpiredError: Error, Equatable, Sendable {
    public init() {}
}

public protocol ProductNetworkPathProviding: Sendable {
    func pathStream() -> AnyAsyncSequence<Bool>
}
