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
    public let resolved: ResolvedProduct
    public let version: ProductVersion
    public let contents: [ProductId: DotNsResolvedContent]

    public init(
        resolved: ResolvedProduct,
        records: [ProductVersion.RecordKey: DotNsSnapshotRecord],
        contents: [ProductId: DotNsResolvedContent]
    ) {
        self.resolved = resolved
        self.contents = contents
        version = ProductVersion(
            productId: resolved.id,
            records: records,
            contentHashes: contents.mapValues(\.contentHash)
        )
    }

    public func content(for kind: ExecutableKind) -> DotNsResolvedContent? {
        contents[resolved.contentId(for: kind)]
    }
}

public struct ProductUpdate: Equatable, Sendable {
    public let previous: ProductVersion
    public let latest: ProductContent

    public init(previous: ProductVersion, latest: ProductContent) {
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
    func updates() -> AnyAsyncSequence<ProductUpdate>
}

public extension ProductContentLoading {
    func open(_ productId: ProductId, policy: ProductLoadPolicy) async throws -> ProductContent {
        try await open(productId, policy: policy, onPhase: { _ in })
    }
}
