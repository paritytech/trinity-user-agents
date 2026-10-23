import AsyncExtensions
import Foundation

public enum ProductLoadPolicy: Equatable, Sendable {
    case untilCancelled
    case attempts(Int)
}

public struct ProductContent: Equatable, Sendable {
    public enum Source: Equatable, Sendable {
        case cache
        case network
    }

    public let resolved: ResolvedProduct
    public let appContent: DotNsResolvedContent
    public let version: ProductVersion
    public let source: Source
}

public struct ProductUpdate: Equatable, Sendable {
    public let previous: ProductVersion
    public let latest: FetchedProductVersion

    public var productId: ProductId { latest.version.productId }
}

public protocol ProductContentLoading: Sendable {
    func open(_ productId: ProductId, policy: ProductLoadPolicy) async throws -> ProductContent
    func refresh(_ productId: ProductId) async -> ProductUpdate?
    func apply(_ update: ProductUpdate) async
    func updates() -> AnyAsyncSequence<ProductUpdate>
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
