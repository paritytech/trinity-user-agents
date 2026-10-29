import Foundation
import Individuality
import MessageExchangeKit
import StatementStore
import Testing
@testable import polkadot_app

struct ChatNotificationSignerResolverTests {
    private let allocator = StubNotificationAllocator()
    private let signerManager = ChatSignerManager(entropyManager: FixedRootEntropyManager())
    private let fallback = FakeIdentitySigningFallback(signer: StubStatementSigner(accountId: Data(
        repeating: 0x0C,
        count: 32
    )))
    private let starved = RecordingIssueDiagnostic()

    @Test func signsWithPeriodAccountOnceItsSlotIsClaimed() async throws {
        let expected = try oneShot("request", period: allocator.period)

        let signer = try await resolve(["request"])["request"]?.get()

        #expect(signer?.accountId == expected)
        #expect(allocator.initiatedTargets == [expected])
        #expect(fallback.contacts.isEmpty)
    }

    @Test func signsWithUsernameWhenNoSlotIsFree() async throws {
        allocator.hasFreeSlot = false

        let signer = try await resolve(["request"])["request"]?.get()

        #expect(signer?.accountId == fallback.signer.accountId)
    }

    @Test func fallsBackToUsernameWhenTheClaimIsDropped() async throws {
        allocator.awaitResult = .failure(NotificationAllocationError.noFreeSlotInPeriod)

        let signer = try await resolve(["request"])["request"]?.get()

        #expect(signer?.accountId == fallback.signer.accountId)
    }

    @Test func declinedUsernameIsATerminalFailure() async throws {
        allocator.hasFreeSlot = false
        fallback.decline()

        let result = try await resolve(["request"])["request"]

        #expect(throws: ChatSigningConsentError.declined) { try result?.get() }
        #expect(starved.failureCount == 0)
    }

    @Test func claimTimeoutIsReturnedWithoutFallback() async throws {
        allocator.awaitResult = .failure(NotificationAllocationError.timeout)

        let result = try await resolve(["request"])["request"]

        #expect(throws: NotificationAllocationError.timeout) { try result?.get() }
        #expect(fallback.contacts.isEmpty)
    }

    @Test func newestRequestWinsTheOnlyFreeSlot() async throws {
        allocator.freeSlotCount = 1

        let resolved = try await resolve(["newer", "older"])

        #expect(try resolved["newer"]?.get().accountId == oneShot("newer", period: allocator.period))
        #expect(try resolved["older"]?.get().accountId == fallback.signer.accountId)
        #expect(try allocator.initiatedTargets == [oneShot("newer", period: allocator.period)])
    }

    @Test func requestLeftWithoutAnySignerIsReportedAsStarved() async throws {
        allocator.hasFreeSlot = false
        fallback.fail(with: NotificationAllocationError.timeout)

        _ = try await resolve(["request"])

        #expect(starved.failureCount == 1)
    }

    @Test func derivesDistinctOneShotAccountsPerRequestAndPeriod() throws {
        let first = try oneShot("request", period: 100)

        #expect(try oneShot("request", period: 101) != first)
        #expect(try oneShot("other", period: 100) != first)
    }
}

private extension ChatNotificationSignerResolverTests {
    func resolve(_ ids: [String]) async throws -> [String: Result<StatementStoreSigning, Error>] {
        let resolver = ChatNotificationSignerResolver<OutgoingChatRequestInput>(
            allocator: allocator,
            signerManager: signerManager,
            fallback: fallback,
            renewalStarved: starved,
            logger: MockLogger()
        )

        return try await resolver.resolve(ids.map(input), period: allocator.period)
    }

    func oneShot(_ id: String, period: UInt32) throws -> Data {
        try signerManager.makeSigner(for: input(id).oneShotKeyId(period: period)).accountId
    }

    func input(_ id: String) -> OutgoingChatRequestInput {
        OutgoingChatRequestInput(
            message: Chat.RequestMessage(
                messageId: id,
                timestamp: 1,
                content: .v1(Chat.RequestContentV1(pushToken: nil, welcomeMessage: nil))
            ),
            session: MessageExchange.SessionRequest(
                own: .init(signKeyId: "//wallet//main", encryptionKeyId: "", pin: nil),
                peer: .init(accountId: Data(repeating: 0x0A, count: 32), publicKey: Data(), pin: nil, devices: [])
            )
        )
    }
}
