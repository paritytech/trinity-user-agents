import AsyncExtensions
import Foundation

public enum ProductLoadPolicy: Equatable, Sendable {
    case untilCancelled
    case attempts(Int)
}

public enum ProductLoadPhase: Equatable, Sendable {
    case resolving
    case downloading(downloadedBytes: Int64, totalBytes: Int64?)
    case unpacking
    case waitingForNetwork
    case retrying(attempt: Int)
}

public typealias ProductLoadPhaseHandler = @Sendable (ProductLoadPhase) -> Void

public struct ProductContent: Equatable, Sendable {
    public enum Source: Equatable, Sendable {
        case cache
        case network
    }

    public let resolved: ResolvedProduct
    public let appContent: DotNsResolvedContent
    public let version: ProductVersion
    public let source: Source

    public init(resolved: ResolvedProduct, appContent: DotNsResolvedContent, version: ProductVersion, source: Source) {
        self.resolved = resolved
        self.appContent = appContent
        self.version = version
        self.source = source
    }
}

public struct ProductUpdate: Equatable, Sendable {
    public let previous: ProductVersion
    public let latest: FetchedProductVersion

    public init(previous: ProductVersion, latest: FetchedProductVersion) {
        self.previous = previous
        self.latest = latest
    }

    public var productId: ProductId { latest.version.productId }
}

public protocol ProductContentLoading: Sendable {
    func open(
        _ productId: ProductId,
        policy: ProductLoadPolicy,
        onPhase: @escaping ProductLoadPhaseHandler
    ) async throws -> ProductContent
    func refresh(_ productId: ProductId) async -> ProductUpdate?
    func apply(_ update: ProductUpdate) async
    func pendingUpdates() async -> [ProductUpdate]
    func removeSupersededContent(keeping servedHashes: Set<String>) async
    func updates() -> AnyAsyncSequence<ProductUpdate>
}

public extension ProductContentLoading {
    func open(_ productId: ProductId, policy: ProductLoadPolicy) async throws -> ProductContent {
        try await open(productId, policy: policy, onPhase: { _ in })
    }
}

public protocol ProductNetworkPathProviding: Sendable {
    func pathStream() -> AnyAsyncSequence<Bool>
}
