import Foundation
import SubstrateSdk
import SubstrateSdkExt
import SubstrateOperation

public protocol ResourcesParametersProviding: Sendable {
    func stmtStoreSlotsPerPeriod(chainId: ChainId, origin: PersonOrigin) async throws -> UInt32
    func stmtStoreReplacementCooldown(chainId: ChainId) async throws -> UInt32
    func longTermStorageClaimsPerPeriod(chainId: ChainId) async throws -> UInt8
    /// Highest claimable notification seq for `origin`'s collection; seqs `0...value` are valid.
    func notificationHighestSeq(chainId: ChainId, origin: PersonOrigin) async throws -> UInt8
}

public enum ResourcesParametersError: Error {
    case valueOutOfRange(functionName: String, value: UInt32)
}

/// Caches the parameters for `ttl` seconds so a burst of allowance operations costs one
/// `state_call` per parameter instead of one per operation.
public actor CachedResourcesParametersProvider {
    public static let defaultTtl: TimeInterval = 300

    private struct CacheKey: Hashable {
        let chainId: ChainId
        let functionName: String
    }

    private struct Cached {
        let value: UInt32
        let storedAt: Date
    }

    private let viewFunctionExecutor: ViewFunctionExecuting
    private let ttl: TimeInterval

    private var cache: [CacheKey: Cached] = [:]
    private var inflight: [CacheKey: Task<UInt32, Error>] = [:]

    public init(viewFunctionExecutor: ViewFunctionExecuting, ttl: TimeInterval = defaultTtl) {
        self.viewFunctionExecutor = viewFunctionExecutor
        self.ttl = ttl
    }
}

extension CachedResourcesParametersProvider: ResourcesParametersProviding {
    public func stmtStoreSlotsPerPeriod(chainId: ChainId, origin: PersonOrigin) async throws -> UInt32 {
        let viewFunction =
            switch origin {
            case .lite: ResourcesPallet.ViewFunction.liteStmtStoreSlotsPerPeriod
            case .full: ResourcesPallet.ViewFunction.stmtStoreSlotsPerPeriod
            }
        return try await value(of: viewFunction, chainId: chainId)
    }

    public func stmtStoreReplacementCooldown(chainId: ChainId) async throws -> UInt32 {
        try await value(of: .stmtStoreReplacementCooldown, chainId: chainId)
    }

    public func longTermStorageClaimsPerPeriod(chainId: ChainId) async throws -> UInt8 {
        let viewFunction = ResourcesPallet.ViewFunction.longTermStorageClaimsPerPeriod
        let value = try await value(of: viewFunction, chainId: chainId)

        guard let claims = UInt8(exactly: value) else {
            throw ResourcesParametersError.valueOutOfRange(functionName: viewFunction.name, value: value)
        }

        return claims
    }

    public func notificationHighestSeq(chainId: ChainId, origin: PersonOrigin) async throws -> UInt8 {
        let viewFunction: ResourcesPallet.ViewFunction =
            switch origin {
            case .lite: .liteNotificationSlotsPerPeriod
            case .full: .notificationSlotsPerPeriod
            }
        let value = try await value(of: viewFunction, chainId: chainId)

        guard let highestSeq = UInt8(exactly: value) else {
            throw ResourcesParametersError.valueOutOfRange(functionName: viewFunction.name, value: value)
        }

        return highestSeq
    }
}

private extension CachedResourcesParametersProvider {
    func value(of viewFunction: ResourcesPallet.ViewFunction, chainId: ChainId) async throws -> UInt32 {
        let key = CacheKey(chainId: chainId, functionName: viewFunction.name)

        if let cached = cache[key], Date().timeIntervalSince(cached.storedAt) < ttl {
            return cached.value
        }

        if let existing = inflight[key] {
            return try await existing.value
        }

        let task = Task { [viewFunctionExecutor] in
            let response: StringCodable<UInt32> = try await viewFunctionExecutor.call(
                viewFunction: viewFunction(),
                chainId: chainId
            )
            return response.wrappedValue
        }
        inflight[key] = task
        defer { inflight[key] = nil }

        let value = try await task.value
        cache[key] = Cached(value: value, storedAt: Date())

        return value
    }
}
