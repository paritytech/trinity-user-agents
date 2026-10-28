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
    public let kind: ExecutableKind
    public let archive: DotNsResolvedContent

    public init(resolved: ResolvedProduct, kind: ExecutableKind, archive: DotNsResolvedContent) {
        self.resolved = resolved
        self.kind = kind
        self.archive = archive
    }

    public var contentId: ProductId {
        resolved.contentId(for: kind)
    }
}

/// A nil `archive` removes the applied one; `record` is nil for a legacy product.
public struct ExecutableRelease: Equatable, Sendable {
    public let contentId: ProductId
    public let archive: DotNsResolvedContent?
    public let record: ProductManifest.Record?

    public init(contentId: ProductId, archive: DotNsResolvedContent?, record: ProductManifest.Record? = nil) {
        self.contentId = contentId
        self.archive = archive
        self.record = record
    }
}

public enum ProductContentError: Error, Equatable {
    case executableNotPublished(ExecutableKind)
}

public protocol ProductContentLoading: Sendable {
    func open(
        _ productId: ProductId,
        kind: ExecutableKind,
        policy: ProductLoadPolicy,
        onPhase: @escaping ProductLoadPhaseHandler
    ) async throws -> ProductContent
    /// A failed worker download keeps the declared worker instead of falling back to legacy.
    func openWorker(_ productId: ProductId) async throws -> ResolvedProduct
    /// Covers only applied executables; the rest are downloaded when first opened.
    func refresh(_ productId: ProductId) async -> [ExecutableRelease]
    func apply(_ release: ExecutableRelease) async
    func updates() -> AnyAsyncSequence<ExecutableRelease>
}

public extension ProductContentLoading {
    func open(_ productId: ProductId, kind: ExecutableKind, policy: ProductLoadPolicy) async throws -> ProductContent {
        try await open(productId, kind: kind, policy: policy, onPhase: { _ in })
    }
}
