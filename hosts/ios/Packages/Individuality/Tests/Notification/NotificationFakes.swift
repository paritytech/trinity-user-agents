import AsyncExtensions
import BandersnatchApi
import DurableTransactions
import DurableTransactionsTestSupport
import ExtrinsicService
import Foundation
import IssueMonitoring
import KeyDerivation
import os
import SubstrateSdk
@testable import Individuality

// MARK: - ContextAliasKeyManager

/// Derives a distinct alias per context, so each seq of each collection is a distinct alias.
final class ContextAliasKeyManager: BandersnatchKeyManaging {
    private let tag: UInt8

    init(tag: UInt8) {
        self.tag = tag
    }

    func getRawPublicKey() throws -> Data {
        Data(repeating: tag, count: 32)
    }

    func sign(_: Data) throws -> Data {
        Data(repeating: 0x02, count: 64)
    }

    func createProof(
        _: Data,
        members _: [Data],
        context _: Data,
        domainSize _: BandersnatchApi.RingDomainSize
    ) throws -> Data {
        Data(repeating: 0x03, count: 128)
    }

    func deriveAlias(for context: Data) throws -> Data {
        Data([tag]) + context
    }
}

// MARK: - Persons

enum NotificationPersons {
    static let full = PersonOrigin.full(3, ContextAliasKeyManager(tag: 0xF0))
    static let lite = PersonOrigin.lite(7, ContextAliasKeyManager(tag: 0x1E))
    static let networkSuffix = Data("paseo".utf8)

    static func alias(of origin: PersonOrigin, period: UInt32, seq: UInt8) throws -> Data {
        let context = try ProductContextSuffix
            .notificationSlot(period: period, seq: seq)
            .context(networkSuffix: networkSuffix)

        return try origin.keyManager.deriveAlias(for: context)
    }
}

final class FixedOriginPersonProvider: OriginPersonProviding {
    var origins: [PersonOrigin]

    init(origins: [PersonOrigin]) {
        self.origins = origins
    }

    func pickPersonOrigin() async throws -> PersonOrigin {
        guard let origin = origins.first else { throw OriginPersonProviderError.noPersonsExist }
        return origin
    }

    func pickPersonOrigins() async throws -> [PersonOrigin] {
        guard !origins.isEmpty else { throw OriginPersonProviderError.noPersonsExist }
        return origins
    }
}

// MARK: - FakeNotificationSlotRepository

struct FixedNetworkSuffixProvider: NetworkSuffixProviding {
    func networkSuffix() async throws -> Data {
        NotificationPersons.networkSuffix
    }
}

final class FakeStatementStoreAllowanceRepository: StatementStoreAllowanceRepositoryProtocol, @unchecked Sendable {
    var allowances: Result<[AccountId: Bool], Error> = .success([:])

    func hasAllowance(_: [AccountId], at _: Data) async throws -> [AccountId: Bool] {
        try allowances.get()
    }
}

final class FakeNotificationSlotRepository: NotificationSlotRepositoryProtocol, @unchecked Sendable {
    var supported = true
    var highestSeqByCollection: [MembersPallet.CollectionIdentifier: UInt8] = [:]
    var registered: Set<Data> = []

    func isSupported() async throws -> Bool {
        supported
    }

    func highestSeq(for origin: PersonOrigin) async throws -> UInt8 {
        highestSeqByCollection[origin.collectionIdentifier] ?? 0
    }

    func registeredAliases(_ aliases: [Data]) async throws -> Set<Data> {
        registered.intersection(aliases)
    }

    func register(_ origin: PersonOrigin, period: UInt32, seq: UInt8) throws {
        try registered.insert(NotificationPersons.alias(of: origin, period: period, seq: seq))
    }
}

// MARK: - FixedNotificationParameters

struct FixedNotificationParameters: NotificationParametersProviding {
    var period: UInt32 = 100
    var statementSize = 10 * 1_024

    func currentPeriod() async throws -> UInt32 {
        period
    }

    func maxStatementSize() async throws -> Int {
        statementSize
    }
}

// MARK: - FakeClaimLedger

final class FakeClaimLedger: NotificationClaimLedger, @unchecked Sendable {
    private let state = OSAllocatedUnfairLock<[DurableTxGroupId: [DurableTxEntry]]>(initialState: [:])
    private let subject = AsyncCurrentValueSubject<[DurableTxGroupId: [DurableTxEntry]]>([:])
    private(set) var scheduled: [(groupId: DurableTxGroupId, policy: SubmissionPolicy)] = []

    func scheduleClaim(groupId: DurableTxGroupId, policy: SubmissionPolicy) async throws {
        scheduled.append((groupId, policy))
        append(.fixture(domainId: NotificationSlotDomain.domainId, status: .pendingSubmission, groupId: groupId))
    }

    func claims(groupId: DurableTxGroupId) async throws -> [DurableTxEntry] {
        state.withLock { $0[groupId] ?? [] }
    }

    func subscribeClaims(groupId: DurableTxGroupId) -> AnyAsyncSequence<[DurableTxEntry]> {
        subject.map { $0[groupId] ?? [] }.eraseToAnyAsyncSequence()
    }

    func setClaims(for target: AccountId, statuses: [DurableTxStatus]) {
        let groupId = NotificationSlotDomain.groupId(for: target)
        let entries = statuses.map {
            DurableTxEntry.fixture(domainId: NotificationSlotDomain.domainId, status: $0, groupId: groupId)
        }
        subject.send(state.withLock { $0[groupId] = entries; return $0 })
    }

    private func append(_ entry: DurableTxEntry) {
        guard let groupId = entry.groupId else { return }
        subject.send(state.withLock { $0[groupId, default: []].append(entry); return $0 })
    }
}

// MARK: - FakeDurableTxMaking

final class FakeDurableTxMaking: DurableTxMaking, @unchecked Sendable {
    private(set) var builtRequests = 0

    func makeExtrinsics(_ requests: [DurableTxRequest], chainId _: ChainId) async throws -> [ExtrinsicBuiltModel] {
        builtRequests += requests.count
        return requests.indices.map { .fixture(payload: "claim-\(builtRequests)-\($0)") }
    }
}

// MARK: - Helpers

extension NotificationSlotDependencies {
    static func test(
        repository: NotificationSlotRepositoryProtocol,
        origins: [PersonOrigin] = [NotificationPersons.full, NotificationPersons.lite],
        period: UInt32 = 100,
        issueDiagnostics: NotificationSlotIssueDiagnostics = .make(reporter: NoopIssueReporter())
    ) -> NotificationSlotDependencies {
        NotificationSlotDependencies(
            chainId: "people",
            sources: .test(repository: repository, origins: origins),
            parameters: FixedNotificationParameters(period: period),
            issueDiagnostics: issueDiagnostics,
            logger: FakeLogger()
        )
    }
}

extension NotificationSlotSources {
    static func test(
        repository: NotificationSlotRepositoryProtocol,
        origins: [PersonOrigin] = [NotificationPersons.full, NotificationPersons.lite]
    ) -> NotificationSlotSources {
        NotificationSlotSources(
            repository: repository,
            originPersonProvider: FixedOriginPersonProvider(origins: origins),
            networkSuffixProvider: FixedNetworkSuffixProvider()
        )
    }
}

extension AccountId {
    static func target(_ byte: UInt8) -> AccountId {
        Data(repeating: byte, count: 32)
    }
}
