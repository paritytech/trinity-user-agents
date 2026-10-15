import Testing
import Combine
import CommonService
import KeyDerivation
import SubstrateSdk
import StructuredConcurrency
import EventCenter
@testable import polkadot_app

@Suite("IdentityProfileService")
struct IdentityProfileServiceTests {
    private let storage: MockUsernameStorage
    private let identityService: MockIdentityService
    let wallet: WalletManaging
    let eventCenter: MockEventCenter

    private let stored = Username(value: "alice.22")
    private let onChain = Username(value: "alice")

    init() throws {
        storage = MockUsernameStorage()
        identityService = MockIdentityService()
        wallet = try MockWalletManager.mockedWallet()
        eventCenter = MockEventCenter()
    }

    func createSut() -> (IdentityProfileServiceProtocol & AppEventVisiting) {
        IdentityProfileService(
            usernameStorage: storage,
            identityService: identityService,
            wallet: wallet,
            eventCenter: eventCenter,
            logger: MockLogger()
        )
    }

    @Test("Init seeds profile from storage values")
    func initSeedsProfileFromStorage() async throws {
        storage.username = onChain
        storage.usernameClaimed = true

        var profiles = createSut().observe().makeAsyncIterator()
        let first = try #require(try await profiles.next())

        #expect(first.username == onChain)
        #expect(first.isClaimed)
        #expect(first.rank == .basic)
    }

    @Test("Init emits empty profile when storage is empty")
    func initSeedsEmptyProfileWhenStorageEmpty() async throws {
        var profiles = createSut().observe().makeAsyncIterator()
        let first = try #require(try await profiles.next())

        #expect(first.username == nil)
        #expect(!first.isClaimed)
        #expect(first.rank == .basic)
    }

    @Test("An unclaimed name is claimed when it appears on chain, and the watch then ends")
    func unclaimedNameIsClaimedWhenItAppearsOnChain() async throws {
        storage.username = stored
        storage.usernameClaimed = false

        let sut = createSut()
        var profiles = sut.observe().makeAsyncIterator()
        _ = try await profiles.next()
        await identityService.awaitSubscribe()

        identityService.subject.send(onChain)

        let claimed = try #require(try await profiles.next())
        #expect(claimed.username == onChain)
        #expect(claimed.isClaimed)
        #expect(storage.username == onChain)
        #expect(storage.usernameClaimed)
    }

    @Test("An already claimed name starts no subscription")
    func claimedNameStartsNoSubscription() async throws {
        storage.username = onChain
        storage.usernameClaimed = true

        try await expectNoSubscription(replacing: Username(value: "bob"))
    }

    @Test("No username starts no subscription")
    func noUsernameStartsNoSubscription() async throws {
        try await expectNoSubscription(replacing: nil)
    }

    @Test("SelectedUsernameChanged event emits refreshed profile")
    func processSelectedUsernameChangedEmitsRefreshedProfile() async throws {
        let sut = createSut()
        var profiles = sut.observe().makeAsyncIterator()
        _ = try await profiles.next()

        storage.username = Username(value: "bob")
        sut.processSelectedUsernameChanged(event: SelectedUsernameChanged(username: storage.username))

        #expect(try await profiles.next()?.username == Username(value: "bob"))
    }
}

private extension IdentityProfileServiceTests {
    /// Asserts no subscription is started, without waiting on a clock.
    ///
    /// The service decides whether to subscribe straight after emitting a profile, so a second
    /// emission is proof that the decision for the first has already been made. Driving one and
    /// awaiting it orders the assertion after that decision.
    func expectNoSubscription(replacing username: Username?) async throws {
        let sut = createSut()
        var profiles = sut.observe().makeAsyncIterator()
        _ = try await profiles.next()

        storage.username = username
        sut.processSelectedUsernameChanged(event: SelectedUsernameChanged(username: username))
        _ = try await profiles.next()

        #expect(identityService.subscribeCallCount == 0)
    }
}

private final class MockUsernameStorage: UsernameStoring, @unchecked Sendable {
    var username: Username?
    var usernameClaimed: Bool = false
}

private final class MockIdentityService: IdentityServiceProtocol, @unchecked Sendable {
    let subject = CurrentValueSubject<Username?, Error>(nil)
    private(set) var subscribeCallCount = 0

    private let continuation: AsyncStream<Void>.Continuation
    private var subscribes: AsyncStream<Void>.AsyncIterator

    init() {
        let (stream, continuation) = AsyncStream.makeStream(of: Void.self)
        self.continuation = continuation
        subscribes = stream.makeAsyncIterator()
    }

    /// Returns once the service has subscribed, so a test can send without racing the subscription.
    func awaitSubscribe() async {
        _ = await subscribes.next()
    }

    func subscribe(to _: AccountId) -> AnyPublisher<Username?, Error> {
        subscribeCallCount += 1
        continuation.yield()
        return subject.eraseToAnyPublisher()
    }

    func username(for _: AccountId) -> AnyPublisher<Username?, Error> {
        Just<Username?>(nil).setFailureType(to: Error.self).eraseToAnyPublisher()
    }
}
