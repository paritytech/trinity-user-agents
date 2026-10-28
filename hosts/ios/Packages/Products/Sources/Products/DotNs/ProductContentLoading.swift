import struct TrUAPIHost.ProductId
import enum TrUAPIHost.ExecutableKind
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
    public let contents: [ProductId: DotNsResolvedContent]

    public init(
        resolved: ResolvedProduct,
        contents: [ProductId: DotNsResolvedContent]
    ) {
        self.resolved = resolved
        self.contents = contents
    }

    public func content(for kind: ExecutableKind) throws -> DotNsResolvedContent? {
        contents[try resolved.contentId(for: kind)]
    }
}

public protocol ProductContentLoading: Sendable {
    func open(
        _ productId: ProductId,
        policy: ProductLoadPolicy,
        onPhase: @escaping ProductLoadPhaseHandler
    ) async throws -> ProductContent
}

public extension ProductContentLoading {
    func open(_ productId: ProductId, policy: ProductLoadPolicy) async throws -> ProductContent {
        try await open(productId, policy: policy, onPhase: { _ in })
    }
}
