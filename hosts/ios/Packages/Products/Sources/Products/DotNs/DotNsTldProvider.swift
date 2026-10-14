import Foundation
import Individuality
import os
import SDKLogger
import StructuredConcurrency

public protocol DotNsTldStoring: Sendable {
    func loadTld() -> String?
    func saveTld(_ tld: String)
}

/// Single chain read of the network's TLD label, without the leading dot.
public protocol DotNsTldReading: Sendable {
    func readTld() async throws -> String
}

public protocol DotNsTldProviding: NetworkSuffixProviding {
    /// Cached TLD label without the leading dot, or nil until a chain read has succeeded.
    /// Kicks a background refresh when nil and the backoff window has elapsed.
    func currentTld() -> String?

    /// Resolves the TLD, sharing any in-flight read. Bypasses the backoff window,
    /// subject to a minimum inter-attempt floor.
    func resolveTld() async throws -> String

    /// Kicks a background re-read of the TLD, subject to the backoff window. Non-blocking.
    func refresh()

    /// Forgets the cached and persisted-at-launch TLD, so the next access reads the chain again.
    /// A read in flight when this is called neither caches nor persists its result.
    func reset()
}

public extension DotNsTldProviding {
    func networkSuffix() async throws -> Data {
        try await Data(resolveTld().utf8)
    }
}

public enum DotNsTldProviderError: Error {
    case resetDuringRead
}

public final class DotNsTldProvider: DotNsTldProviding {
    private let reader: DotNsTldReading
    private let now: @Sendable () -> Date
    private let store: DotNsTldStoring?
    private let logger: SDKLoggerProtocol?
    private let state: OSAllocatedUnfairLock<State>
    private let coalescer = CoalescingTask<String>()

    private static let minimumInterAttemptInterval: TimeInterval = 1
    private static let maximumBackoff: TimeInterval = 60

    private struct State {
        var tld: String?
        var persistedTld: String?
        /// Bumped by reset(), so a read started before it cannot write its result back.
        var generation: Int = 0
        var failureCount: Int = 0
        var nextAttemptAt: Date = .distantPast
        var nextStartAllowedAt: Date = .distantPast
    }

    public init(
        reader: DotNsTldReading,
        store: DotNsTldStoring? = nil,
        logger: SDKLoggerProtocol? = nil,
        now: @Sendable @escaping () -> Date = { Date() }
    ) {
        self.reader = reader
        self.store = store
        self.logger = logger
        let persistedTld = store?.loadTld()
        state = OSAllocatedUnfairLock(initialState: State(persistedTld: persistedTld))
        self.now = now
        logger?.debug("DotNs TLD provider started, persisted: \(persistedTld ?? "none")")
    }

    public func currentTld() -> String? {
        let (tld, persistedTld) = state.withLock { ($0.tld, $0.persistedTld) }
        if let tld { return tld }
        refreshIfNeeded()
        return persistedTld
    }

    public func resolveTld() async throws -> String {
        if let tld = state.withLock({ $0.tld }) { return tld }

        return try await coalescer.run { [self] in
            // A joiner arriving after a successful read must not trigger a second one.
            if let tld = state.withLock({ $0.tld }) { return tld }
            guard claimStart(ignoringBackoff: true) else {
                logger?.error("DotNs TLD resolve rejected by inter-attempt floor")
                throw DotNsContractError.tldNotFound
            }
            return try await performRead()
        }
    }

    public func refresh() {
        refreshIfNeeded()
    }

    public func reset() {
        state.withLock { state in
            state = State(generation: state.generation + 1)
        }
        logger?.debug("DotNs TLD provider reset")
    }
}

private extension DotNsTldProvider {
    /// Reserves the right to start a chain read, applying the minimum inter-attempt
    /// floor and (unless bypassed) the failure backoff window.
    /// - Returns: `true` when the caller may proceed with a read.
    func claimStart(ignoringBackoff: Bool) -> Bool {
        state.withLock { state in
            let currentTime = now()
            guard currentTime >= state.nextStartAllowedAt else { return false }
            guard ignoringBackoff || currentTime >= state.nextAttemptAt else { return false }
            state.nextStartAllowedAt = currentTime.addingTimeInterval(Self.minimumInterAttemptInterval)
            return true
        }
    }

    func performRead() async throws -> String {
        logger?.debug("DotNs TLD chain read started")

        let generation = state.withLock { $0.generation }
        let tld: String

        do {
            tld = try await reader.readTld()
        } catch {
            finish(.failure(error), generation: generation)
            logger?.error("DotNs TLD read failed: \(error)")
            throw error
        }

        guard finish(.success(tld), generation: generation) else {
            logger?.debug("DotNs TLD read discarded after reset: \(tld)")
            throw DotNsTldProviderError.resetDuringRead
        }

        logger?.debug("DotNs TLD resolved: \(tld)")
        return tld
    }

    func refreshIfNeeded() {
        guard claimStart(ignoringBackoff: false) else { return }
        Task(priority: .utility) { [weak self] in
            guard let provider = self else { return }
            _ = try? await provider.coalescer.run {
                if let tld = provider.state.withLock({ $0.tld }) { return tld }
                return try await provider.performRead()
            }
        }
    }

    /// - Returns: `false` when a reset happened since the read started, leaving the state and store untouched.
    @discardableResult
    func finish(_ result: Result<String, Error>, generation: Int) -> Bool {
        state.withLock { state in
            guard state.generation == generation else { return false }

            switch result {
            case let .success(tld):
                state.tld = tld
                state.failureCount = 0
                state.nextAttemptAt = .distantPast
                // Saved under the lock so a concurrent reset() cannot land between caching and persisting.
                store?.saveTld(tld)
            case .failure:
                state.failureCount += 1
                let backoff = min(pow(2.0, Double(state.failureCount)), Self.maximumBackoff)
                state.nextAttemptAt = now().addingTimeInterval(backoff)
            }

            return true
        }
    }
}
