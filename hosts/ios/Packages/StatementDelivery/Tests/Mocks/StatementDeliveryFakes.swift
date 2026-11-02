import AsyncExtensions
import BackgroundExecution
import Foundation
import FoundationExt
import IssueMonitoring
import os
import SDKLogger
import StatementDelivery
import StatementStore
import SubstrateSdk

struct TestInput: OutgoingStatementInput, Equatable {
    let statementId: String
    var identitySignKeyId: String { "identity" }

    func oneShotKeyId(period: UInt32) throws -> String {
        "one-shot/\(statementId)/\(period)"
    }
}

struct StubSigner: StatementStoreSigning {
    let accountId: AccountId

    init(keyId: String) {
        accountId = Data(keyId.utf8).paddedTo32()
    }

    func sign(_: Data) throws -> StatementProof {
        .sr25519(signature: Data(repeating: 0, count: StatementProof.signatureSize), signer: accountId)
    }
}

final class StubSignerManager: StatementStoreSignerManaging {
    func makeSigner(for signerKeyId: String) throws -> StatementStoreSigning {
        StubSigner(keyId: signerKeyId)
    }
}

struct TestTerminalError: OutgoingStatementTerminalError {}
struct TestTransientError: Error {}

final class FakeOutgoingStore: OutgoingStatementStoring, @unchecked Sendable {
    private struct State {
        var awaitingFirst: Set<String> = []
        var candidates: [PublishedStatement<TestInput>] = []
        var acknowledged: Set<String> = []
        var published: [(id: String, period: UInt32)] = []
        var publishedOnce: Set<String> = []
        var failed: [String] = []
        var markedAcknowledged: [String] = []
        var abandoned: [String] = []
    }

    private let state = OSAllocatedUnfairLock(uncheckedState: State())

    var published: [(id: String, period: UInt32)] { state.withLock { $0.published } }
    var failed: [String] { state.withLock { $0.failed } }
    var markedAcknowledged: [String] { state.withLock { $0.markedAcknowledged } }
    var abandoned: [String] { state.withLock { $0.abandoned } }

    func awaitFirstDelivery(of id: String) {
        state.withLock { _ = $0.awaitingFirst.insert(id) }
    }

    func setCandidates(_ candidates: [PublishedStatement<TestInput>]) {
        state.withLock { state in
            state.candidates = candidates
            candidates.forEach { state.publishedOnce.insert($0.input.statementId) }
        }
    }

    func acknowledge(_ id: String) {
        state.withLock { _ = $0.acknowledged.insert(id) }
    }

    func isAwaitingFirstDelivery(_ input: TestInput) async throws -> Bool {
        state.withLock { $0.awaitingFirst.contains(input.statementId) }
    }

    func renewalCandidates() async throws -> [PublishedStatement<TestInput>] {
        state.withLock { $0.candidates }
    }

    func markPublished(_ input: TestInput, period: UInt32) async throws -> StatementPublication {
        state.withLock { state in
            state.published.append((input.statementId, period))
            state.awaitingFirst.remove(input.statementId)
            let isFirst = state.publishedOnce.insert(input.statementId).inserted
            return StatementPublication(period: period, isFirst: isFirst)
        }
    }

    func markFailed(_ input: TestInput, failure _: OutgoingStatementFailure) async throws {
        state.withLock { state in
            state.failed.append(input.statementId)
            state.awaitingFirst.remove(input.statementId)
        }
    }

    func isAcknowledged(_ input: TestInput) async throws -> Bool {
        state.withLock { $0.acknowledged.contains(input.statementId) }
    }

    func markAcknowledged(_ input: TestInput) async throws {
        state.withLock { $0.markedAcknowledged.append(input.statementId) }
    }

    func markAbandoned(_ input: TestInput) async throws {
        state.withLock { $0.abandoned.append(input.statementId) }
    }
}

final class FakeResolver: StatementSignerResolving, @unchecked Sendable {
    private struct State {
        var period: UInt32 = 10
        var maxSize = 1_000
        var failures: [String: Error] = [:]
        var calls: [[String]] = []
    }

    private let state = OSAllocatedUnfairLock(uncheckedState: State())

    var calls: [[String]] { state.withLock { $0.calls } }

    func setPeriod(_ period: UInt32) { state.withLock { $0.period = period } }
    func setMaxSize(_ size: Int) { state.withLock { $0.maxSize = size } }
    func fail(_ id: String, with error: Error) { state.withLock { $0.failures[id] = error } }

    func signerAccount(for id: String, period: UInt32) -> AccountId {
        StubSigner(keyId: "one-shot/\(id)/\(period)").accountId
    }

    func currentPeriod() async throws -> UInt32 { state.withLock { $0.period } }

    func maxStatementSize() async throws -> Int { state.withLock { $0.maxSize } }

    func resolve(_ inputs: [TestInput], period: UInt32) async throws -> [String: Result<StatementStoreSigning, Error>] {
        state.withLock { state in
            state.calls.append(inputs.map(\.statementId))

            return inputs.reduce(into: [:]) { result, input in
                if let failure = state.failures[input.statementId] {
                    result[input.statementId] = .failure(failure)
                } else {
                    result[input.statementId] = .success(StubSigner(keyId: "one-shot/\(input.statementId)/\(period)"))
                }
            }
        }
    }
}

final class FakeOutgoingService: OutgoingStatementServicing, @unchecked Sendable {
    private struct State {
        var publishes: [(id: String, signer: AccountId)] = []
        var failuresLeft = 0
        var stored: Set<String> = []
        var holdsPublish = false
        var released = false
        var entered = 0
    }

    private let state = OSAllocatedUnfairLock(uncheckedState: State())

    var publishes: [(id: String, signer: AccountId)] { state.withLock { $0.publishes } }

    func failNext(_ times: Int) { state.withLock { $0.failuresLeft = times } }
    func markStored(_ id: String) { state.withLock { _ = $0.stored.insert(id) } }
    func holdPublishes() { state.withLock { $0.holdsPublish = true } }
    func release() { state.withLock { $0.released = true } }

    func waitUntilPublishStarted() async {
        while state.withLock({ $0.entered == 0 }) {
            await Task.yield()
        }
    }

    func publish(_ input: TestInput, signer: StatementStoreSigning) async throws {
        state.withLock { $0.entered += 1 }

        while state.withLock({ $0.holdsPublish && !$0.released }) {
            await Task.yield()
        }

        let shouldFail = state.withLock { state in
            guard state.failuresLeft > 0 else { return false }
            state.failuresLeft -= 1
            return true
        }

        if shouldFail { throw TestTransientError() }

        state.withLock { $0.publishes.append((input.statementId, signer.accountId)) }
    }

    func isStored(_ input: TestInput, period _: UInt32) async throws -> Bool {
        state.withLock { $0.stored.contains(input.statementId) }
    }
}

struct FakeBuilder: OutgoingStatementBuilding {
    var size = 100

    func makeUnsignedStatement(for input: TestInput) throws -> UnsignedStatement {
        UnsignedStatement().addTopic1(Data(input.statementId.utf8).paddedTo32())
    }

    func encodedSize(of _: TestInput) throws -> Int { size }

    func storageTopicFilter(for input: TestInput) throws -> TopicFilter {
        .matchAll([Data(input.statementId.utf8).paddedTo32()])
    }
}

final class RecordingPostDelivery: OutgoingStatementPostDelivering, @unchecked Sendable {
    private let calls = OSAllocatedUnfairLock<[String]>(initialState: [])

    var delivered: [String] { calls.withLock { $0 } }

    func didPublishFirstTime(_ input: TestInput) async {
        calls.withLock { $0.append(input.statementId) }
    }
}

struct InlineExecutor: BackgroundExecuting {
    func execute<T: Sendable>(_ operation: @escaping @Sendable () async throws -> T) async throws -> T {
        try await operation()
    }
}

final class SilentLogger: SDKLoggerProtocol {
    func verbose(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func debug(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func info(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func warning(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func error(message _: () -> String, file _: String, function _: String, line _: Int) {}
}

extension OutgoingStatementDiagnostics {
    static var silent: Self {
        OutgoingStatementDiagnostics(
            logger: SilentLogger(),
            issues: OutgoingStatementIssueDiagnostics(
                oversized: NoopIssueDiagnostic(),
                deliveryStalled: NoopIssueDiagnostic(),
                deliveryStateUnreadable: NoopIssueDiagnostic(),
                renewalPublishFailed: NoopIssueDiagnostic(),
                renewalFailing: NoopIssueDiagnostic()
            )
        )
    }
}

extension Data {
    func paddedTo32() -> Data {
        prefix(32) + Data(repeating: 0, count: Swift.max(0, 32 - count))
    }
}
