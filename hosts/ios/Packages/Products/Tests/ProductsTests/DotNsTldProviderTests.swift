import Foundation
import Testing
@testable import Products

// MARK: - Stubs

private final class StubTldReader: DotNsTldReading {
    var readTldCallCount = 0
    var readTldResult: Result<String, any Error> = .failure(DotNsContractError.tldNotFound)
    var yieldsBeforeReturning = false

    func readTld() async throws -> String {
        readTldCallCount += 1

        if yieldsBeforeReturning {
            await Task.yield()
        }

        return try readTldResult.get()
    }
}

private final class StubDotNsTldStore: DotNsTldStoring {
    var loadedTld: String?
    var savedTlds: [String] = []

    func loadTld() -> String? {
        loadedTld
    }

    func saveTld(_ tld: String) {
        savedTlds.append(tld)
    }
}

/// Suspends the first read until `release` is called; later reads return `laterTld` at once.
private final class GatedTldReader: DotNsTldReading {
    let started: AsyncStream<Void>
    private let startedContinuation: AsyncStream<Void>.Continuation
    private let lock = NSLock()
    private var pending: CheckedContinuation<String, Never>?
    private var isFirstRead = true
    var laterTld = ""

    init() {
        (started, startedContinuation) = AsyncStream.makeStream()
    }

    func release(with tld: String) {
        let continuation = lock.withLock {
            defer { pending = nil }
            return pending
        }
        continuation?.resume(returning: tld)
    }

    func readTld() async throws -> String {
        let shouldSuspend = lock.withLock {
            defer { isFirstRead = false }
            return isFirstRead
        }

        guard shouldSuspend else { return laterTld }

        return await withCheckedContinuation { continuation in
            lock.withLock { pending = continuation }
            startedContinuation.yield()
        }
    }
}

/// Test clock whose reading can be moved forward between assertions.
private final class MutableClock {
    var seconds: Double = 0
}

// MARK: - Tests

struct DotNsTldProviderTests {
    @Test func currentTldIsNilBeforeSuccessfulRead() {
        let stub = StubTldReader()
        stub.readTldResult = .success("dot")
        let provider = DotNsTldProvider(reader: stub)

        #expect(provider.currentTld() == nil)
    }

    @Test func currentTldReturnsValueAfterSuccessfulResolve() async throws {
        let stub = StubTldReader()
        stub.readTldResult = .success("dot")
        let provider = DotNsTldProvider(reader: stub)

        _ = try await provider.resolveTld()

        #expect(provider.currentTld() == "dot")
    }

    @Test func resolveTldDoesNotCallReadTldAgainAfterSuccess() async throws {
        let stub = StubTldReader()
        stub.readTldResult = .success("dot")
        let provider = DotNsTldProvider(reader: stub)

        _ = try await provider.resolveTld()
        _ = provider.currentTld()

        #expect(stub.readTldCallCount == 1)
    }

    @Test func multipleConcurrentResolveCallsResultInSingleRead() async throws {
        let stub = StubTldReader()
        stub.readTldResult = .success("dot")
        stub.yieldsBeforeReturning = true
        let provider = DotNsTldProvider(reader: stub)

        async let first = provider.resolveTld()
        async let second = provider.resolveTld()

        let firstTld = try await first
        let secondTld = try await second

        #expect(firstTld == "dot")
        #expect(secondTld == "dot")
        #expect(stub.readTldCallCount == 1)
    }

    @Test func failureLeavesCacheEmpty() async {
        let stub = StubTldReader()
        stub.readTldResult = .failure(DotNsContractError.contentHashNotFound)
        let provider = DotNsTldProvider(reader: stub)

        await #expect(throws: DotNsContractError.self) {
            _ = try await provider.resolveTld()
        }

        #expect(provider.currentTld() == nil)
    }

    @Test func retryAfterBackoffSucceeds() async throws {
        let stub = StubTldReader()
        stub.readTldResult = .failure(DotNsContractError.contentHashNotFound)
        let clock = MutableClock()
        let provider = DotNsTldProvider(
            reader: stub,
            now: { Date(timeIntervalSince1970: clock.seconds) }
        )

        await #expect(throws: DotNsContractError.self) {
            _ = try await provider.resolveTld()
        }

        clock.seconds = 4
        stub.readTldResult = .success("dot")

        let tld = try await provider.resolveTld()

        #expect(tld == "dot")
    }

    @Test func noReadWhileInBackoffWindow() async {
        let stub = StubTldReader()
        stub.readTldResult = .failure(DotNsContractError.contentHashNotFound)
        let clock = MutableClock()
        let provider = DotNsTldProvider(
            reader: stub,
            now: { Date(timeIntervalSince1970: clock.seconds) }
        )

        await #expect(throws: DotNsContractError.self) {
            _ = try await provider.resolveTld()
        }

        let callCountAfterFailure = stub.readTldCallCount

        clock.seconds = 0.5
        _ = provider.currentTld()

        #expect(stub.readTldCallCount == callCountAfterFailure)
    }

    @Test func currentTldReturnsPersistedValueImmediately() {
        let stub = StubTldReader()
        let store = StubDotNsTldStore()
        store.loadedTld = "dot"
        let provider = DotNsTldProvider(reader: stub, store: store)

        #expect(provider.currentTld() == "dot")
    }

    @Test func successfulResolvePersistsValue() async throws {
        let stub = StubTldReader()
        stub.readTldResult = .success("dot")
        let store = StubDotNsTldStore()
        let provider = DotNsTldProvider(reader: stub, store: store)

        _ = try await provider.resolveTld()

        #expect(store.savedTlds == ["dot"])
    }

    @Test func failedReadLeavesPersistedValueInCurrentTld() async {
        let stub = StubTldReader()
        stub.readTldResult = .failure(DotNsContractError.contentHashNotFound)
        let store = StubDotNsTldStore()
        store.loadedTld = "dot"
        let provider = DotNsTldProvider(reader: stub, store: store)

        await #expect(throws: DotNsContractError.self) {
            _ = try await provider.resolveTld()
        }

        #expect(provider.currentTld() == "dot")
    }

    @Test func noStoreNoSuccessfulReadReturnsNil() {
        let stub = StubTldReader()
        let provider = DotNsTldProvider(reader: stub, store: nil)

        #expect(provider.currentTld() == nil)
    }

    @Test func resetForgetsCachedAndPersistedTld() async throws {
        let stub = StubTldReader()
        stub.readTldResult = .success("new")
        let store = StubDotNsTldStore()
        store.loadedTld = "old"
        let provider = DotNsTldProvider(reader: stub, store: store)

        _ = try await provider.resolveTld()
        provider.reset()

        #expect(provider.currentTld() == nil)
    }

    @Test func resolveAfterResetReadsChainAgain() async throws {
        let stub = StubTldReader()
        stub.readTldResult = .success("old")
        let provider = DotNsTldProvider(reader: stub)

        _ = try await provider.resolveTld()
        provider.reset()
        stub.readTldResult = .success("new")

        let tld = try await provider.resolveTld()

        #expect(tld == "new")
        #expect(stub.readTldCallCount == 2)
    }

    @Test func readInFlightDuringResetIsNeitherCachedNorPersisted() async throws {
        let reader = GatedTldReader()
        let store = StubDotNsTldStore()
        let provider = DotNsTldProvider(reader: reader, store: store)

        let staleRead = Task { try await provider.resolveTld() }
        var startedIterator = reader.started.makeAsyncIterator()
        await startedIterator.next()

        provider.reset()
        reader.release(with: "old")

        await #expect(throws: DotNsTldProviderError.resetDuringRead) {
            _ = try await staleRead.value
        }
        #expect(store.savedTlds.isEmpty)

        reader.laterTld = "new"
        let tld = try await provider.resolveTld()

        #expect(tld == "new")
        #expect(store.savedTlds == ["new"])
    }
}
