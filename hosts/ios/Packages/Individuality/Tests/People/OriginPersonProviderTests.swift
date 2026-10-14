import Foundation
import BandersnatchApi
import KeyDerivation
import SubstrateSdk
import Testing
@testable import Individuality

struct OriginPersonProviderTests {
    private static let liteCollection = Data(repeating: 0x1A, count: 4)
    private static let fullCollection = Data(repeating: 0x2B, count: 4)

    private let lite = KeyedBandersnatchKeyManaging(publicKey: Data(repeating: 0x11, count: 32))
    private let full = KeyedBandersnatchKeyManaging(publicKey: Data(repeating: 0x22, count: 32))
    private let checker = StubMembershipStatusChecker()
    private let waiter = StubMembershipStatusWaiter()

    private func makeProvider(strategy: NoPersonStrategy = .default) -> OriginPersonProvider {
        OriginPersonProvider(
            liteVrfManager: lite,
            liteCollectionId: Self.liteCollection,
            fullVrfManager: full,
            fullCollectionId: Self.fullCollection,
            memberStatusChecker: checker,
            liteStatusWaiter: waiter,
            noPersonStrategy: strategy
        )
    }

    @Test("both persons on chain are returned, full first, without waiting")
    func bothPersons() async throws {
        checker.statuses = try [full.getMemberKey(): 3, lite.getMemberKey(): 7]

        let origins = try await makeProvider().pickPersonOrigins()

        #expect(origins.map(\.ringIndex) == [3, 7])
        #expect(waiter.waits.isEmpty)
    }

    @Test("a lite person still registering is waited for and returned once it lands")
    func liteRegistering() async throws {
        waiter.ringIndex = 9

        let origins = try await makeProvider().pickPersonOrigins()

        #expect(origins.count == 1)
        #expect(origins.first?.ringIndex == 9)
        #expect(origins.first.map(\.isLite) == true)
        #expect(waiter.waits.count == 1)
        #expect(try waiter.waits.first?.input.memberKey == lite.getMemberKey())
        #expect(waiter.waits.first?.input.collection == Self.liteCollection)
    }

    @Test("the default strategy waits fifteen seconds")
    func defaultDelay() async throws {
        waiter.ringIndex = 1

        _ = try await makeProvider().pickPersonOrigins()

        #expect(waiter.waits.first?.timeout == .seconds(20))
    }

    @Test("a lite person that never lands within the delay is no person")
    func liteNeverLands() async throws {
        waiter.ringIndex = nil

        await #expect(throws: OriginPersonProviderError.self) {
            try await makeProvider(strategy: .waitLight(.seconds(2))).pickPersonOrigins()
        }
        #expect(waiter.waits.first?.timeout == .seconds(2))
    }

    @Test("the error strategy fails without watching the chain")
    func errorStrategy() async throws {
        waiter.ringIndex = 9

        await #expect(throws: OriginPersonProviderError.self) {
            try await makeProvider(strategy: .noPersonError).pickPersonOrigin()
        }
        #expect(waiter.waits.isEmpty)
    }
}

private extension PersonOrigin {
    var isLite: Bool {
        if case .lite = self { true } else { false }
    }
}

// MARK: - Fakes

private final class KeyedBandersnatchKeyManaging: BandersnatchKeyManaging {
    private let publicKey: Data

    init(publicKey: Data) {
        self.publicKey = publicKey
    }

    func getRawPublicKey() throws -> Data { publicKey }
    func sign(_: Data) throws -> Data { Data() }

    func createProof(
        _: Data,
        members _: [Data],
        context _: Data,
        domainSize _: BandersnatchApi.RingDomainSize
    ) throws -> Data {
        Data()
    }

    func deriveAlias(for _: Data) throws -> Data { publicKey }
}

private final class StubMembershipStatusChecker: MembershipStatusChecking {
    var statuses: [MembersPallet.RingMember: MembersPallet.RingIndex] = [:]

    func checkStatuses(
        of _: [MembershipStatusInput],
        blockHash _: BlockHashData?
    ) async throws -> [MembersPallet.RingMember: MembersPallet.RingIndex] {
        statuses
    }
}

private final class StubMembershipStatusWaiter: MembershipStatusWaiting {
    struct Wait {
        let input: MembershipStatusInput
        let timeout: Duration
    }

    var ringIndex: MembersPallet.RingIndex?
    private(set) var waits: [Wait] = []

    func waitForStatus(of input: MembershipStatusInput, timeout: Duration) async throws -> MembersPallet.RingIndex? {
        waits.append(Wait(input: input, timeout: timeout))
        return ringIndex
    }
}
