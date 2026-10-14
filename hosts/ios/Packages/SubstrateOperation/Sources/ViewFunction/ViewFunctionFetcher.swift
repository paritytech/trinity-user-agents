import Foundation
import FoundationExt
import SubstrateSdk

/// Caches view-function results for a single chain in memory for `ttl` seconds so a burst of reads
/// costs one `state_call` per distinct (function, args) instead of one per read. Concurrent reads
/// of the same key are coalesced onto a single in-flight call.
public protocol ViewFunctionFetching: Sendable {
    func fetch<T: Decodable>(
        viewFunction: ViewFunctionCodingPath,
        args: [ScaleEncodable]
    ) async throws -> T
}

public extension ViewFunctionFetching {
    func fetch<T: Decodable>(
        viewFunction: ViewFunctionCodingPath
    ) async throws -> T {
        try await fetch(viewFunction: viewFunction, args: [])
    }
}

public enum ViewFunctionFetcherError: Error {
    case unexpectedCachedType
}

public actor ViewFunctionFetcher {
    public static let defaultTtl: TimeInterval = 300

    private struct CacheKey: Hashable {
        let moduleName: String
        let functionName: String
        let argsKey: Data
    }

    private struct Cached {
        let value: Any
        let storedAt: Date
    }

    private let executor: ViewFunctionExecuting
    private let chainId: ChainId
    private let ttl: TimeInterval
    private let dateProvider: DateProviding

    private var cache: [CacheKey: Cached] = [:]
    private var inflight: [CacheKey: Task<Any, Error>] = [:]

    public init(
        executor: ViewFunctionExecuting,
        chainId: ChainId,
        ttl: TimeInterval = defaultTtl,
        dateProvider: DateProviding = NowDateProvider()
    ) {
        self.executor = executor
        self.chainId = chainId
        self.ttl = ttl
        self.dateProvider = dateProvider
    }
}

extension ViewFunctionFetcher: ViewFunctionFetching {
    public func fetch<T: Decodable>(
        viewFunction: ViewFunctionCodingPath,
        args: [ScaleEncodable]
    ) async throws -> T {
        let key = try makeKey(viewFunction: viewFunction, args: args)

        let now = await dateProvider.read()
        if let cached = cache[key], now.timeIntervalSince(cached.storedAt) < ttl {
            return try cast(cached.value)
        }

        if let existing = inflight[key] {
            return try await cast(existing.value)
        }

        let task = Task { [executor, chainId] () -> Any in
            let value: T = try await executor.call(viewFunction: viewFunction, chainId: chainId, args: args)
            return value
        }
        inflight[key] = task
        defer { inflight[key] = nil }

        let value = try await task.value
        cache[key] = await Cached(value: value, storedAt: dateProvider.read())

        return try cast(value)
    }
}

extension ViewFunctionFetcher {
    private func makeKey(
        viewFunction: ViewFunctionCodingPath,
        args: [ScaleEncodable]
    ) throws -> CacheKey {
        let encoder = ScaleEncoder()
        try args.forEach { try $0.encode(scaleEncoder: encoder) }

        return CacheKey(
            moduleName: viewFunction.moduleName,
            functionName: viewFunction.functionName,
            argsKey: encoder.encode()
        )
    }

    private func cast<T>(_ value: Any) throws -> T {
        guard let typed = value as? T else {
            throw ViewFunctionFetcherError.unexpectedCachedType
        }
        return typed
    }
}
