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

/// One executable of a product, opened from the archive applied for it.
public struct ProductContent: Equatable, Sendable {
    public let resolved: ResolvedProduct
    public let kind: ExecutableKind
    public let archive: DotNsResolvedContent

    public init(resolved: ResolvedProduct, kind: ExecutableKind, archive: DotNsResolvedContent) {
        self.resolved = resolved
        self.kind = kind
        self.archive = archive
    }

    /// The name `archive` is applied under, which is what an update for it names.
    public var contentId: ProductId {
        resolved.contentId(for: kind)
    }
}

/// A newer archive, already on disk, for an executable whose applied archive differs. A nil
/// archive means the executable is no longer published, so applying it drops the applied one.
/// `record` is the executable's manifest record from the same release, applied with the archive;
/// nil for a legacy product, whose base name has no executable record.
public struct ProductContentUpdate: Equatable, Sendable {
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
    /// Resolves the product and stages its chat worker's archive. A failed archive download
    /// leaves the declared worker in place, so it fails when it runs rather than turning into a
    /// legacy product.
    func openWorker(_ productId: ProductId) async throws -> ResolvedProduct
    /// Re-reads the manifest and every executable the device has applied. Executables it never
    /// opened are not downloaded.
    func refresh(_ productId: ProductId) async -> [ProductContentUpdate]
    func apply(_ update: ProductContentUpdate) async
    func updates() -> AnyAsyncSequence<ProductContentUpdate>
}

public extension ProductContentLoading {
    func open(_ productId: ProductId, kind: ExecutableKind, policy: ProductLoadPolicy) async throws -> ProductContent {
        try await open(productId, kind: kind, policy: policy, onPhase: { _ in })
    }
}
