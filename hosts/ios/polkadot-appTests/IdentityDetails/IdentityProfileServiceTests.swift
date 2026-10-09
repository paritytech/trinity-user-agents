import Testing
import Combine
import CommonService
import KeyDerivation
import SubstrateSdk
import StructuredConcurrency
import EventCenter
import AsyncExtensions
@testable import polkadot_app

@Suite("IdentityProfileService", .timeLimit(.minutes(1)))
struct IdentityProfileServiceTests {
    private let storage: MockUsernameStorage
    private let identityService: MockIdentityService
    let personDataStore: MockObservableStore<DetermineStatePersonData>
    let wallet: WalletManaging
    let eventCenter: MockEventCenter

    init() throws {
        storage = MockUsernameStorage()
        identityService = MockIdentityService()
        personDataStore = MockObservableStore<DetermineStatePersonData>(logger: MockLogger())
        wallet = try MockWalletManager.mockedWallet()
        eventCenter = MockEventCenter()
    }

    func createSut() -> (IdentityProfileServiceProtocol & AppEventVisiting) {
        IdentityProfileService(
            usernameStorage: storage,
            identityService: identityService,
            personDataStore: personDataStore,
            wallet: wallet,
            eventCenter: eventCenter,
            logger: MockLogger()
        )
    }

    @Test("Init seeds profile from storage values")
    func initSeedsProfileFromStorage() async throws {
        storage.username = Username(value: "alice")
        storage.usernameClaimed = true
        storage.isPerson = true

        let sut = createSut()
        let profile = try await sut.observe()
            .first(where: { _ in true })

        let first = try #require(profile)

        #expect(first.username == Username(value: "alice"))
        #expect(first.isClaimed)
        #expect(first.rank == .membership)
    }

    @Test("Init emits empty profile when storage is empty")
    func initSeedsEmptyProfileWhenStorageEmpty() async throws {
        let sut = createSut()
        let profile = try await sut.observe()
            .first(where: { _ in true })

        let first = try #require(profile)

        #expect(first.username == nil)
        #expect(!first.isClaimed)
        #expect(first.rank == .basic)
    }

    @Test("On-chain subscription claims username and emits update")
    func onChainSubscriptionClaimsUsername() async throws {
        storage.username = Username(value: "alice.22")
        storage.usernameClaimed = false
        let sut = createSut()
        var profiles = try await observeAfterFirstProfile(of: sut)

        identityService.subject.send(Username(value: "aliceclaimed"))

        var claimed: IdentityProfile?
        repeat {
            claimed = try await profiles.next()
        } while claimed?.isClaimed == false

        let claimedProfile = try #require(claimed)

        #expect(claimedProfile.username == Username(value: "aliceclaimed"))
        #expect(storage.username == Username(value: "aliceclaimed"))
        #expect(storage.usernameClaimed)
    }

    @Test("On-chain subscription skipped when already claimed")
    func onChainSubscriptionSkippedWhenAlreadyClaimed() async throws {
        storage.username = Username(value: "alice")
        storage.usernameClaimed = true
        let sut = createSut()
        var profiles = try await observeAfterFirstProfile(of: sut)

        try await awaitInitialStateProcessed(by: sut, profiles: &profiles)

        #expect(identityService.subscribeCallCount == 0)
    }

    @Test("On-chain subscription skipped when no username")
    func onChainSubscriptionSkippedWhenNoUsername() async throws {
        let sut = createSut()
        var profiles = try await observeAfterFirstProfile(of: sut)

        try await awaitInitialStateProcessed(by: sut, profiles: &profiles)

        #expect(identityService.subscribeCallCount == 0)
    }

    @Test("SelectedUsernameChanged event emits refreshed profile")
    func processSelectedUsernameChangedEmitsRefreshedProfile() async throws {
        let sut = createSut()
        var profiles = try await observeAfterFirstProfile(of: sut)

        storage.username = Username(value: "bob")
        sut.processSelectedUsernameChanged(event: SelectedUsernameChanged(username: storage.username))

        #expect(try await profiles.next()?.username == Username(value: "bob"))
    }
}

private extension IdentityProfileServiceTests {
    /// Receiving the first profile proves the consumer is registered, so later changes are not missed.
    func observeAfterFirstProfile(
        of sut: IdentityProfileServiceProtocol
    ) async throws -> AnyAsyncIterator<IdentityProfile> {
        var profiles = sut.observe().makeAsyncIterator()
        _ = try await profiles.next()

        return profiles
    }

    /// States are handled in order; a claimed username never subscribes and, unlike `isPerson`, isn't debounced.
    func awaitInitialStateProcessed(
        by sut: IdentityProfileServiceProtocol & AppEventVisiting,
        profiles: inout AnyAsyncIterator<IdentityProfile>
    ) async throws {
        storage.username = Username(value: "barrier")
        storage.usernameClaimed = true
        sut.processSelectedUsernameChanged(event: SelectedUsernameChanged(username: storage.username))

        _ = try await profiles.next()
    }
}

private final class MockUsernameStorage: UsernameStoring, @unchecked Sendable {
    var username: Username?
    var usernameClaimed: Bool = false
    var isPerson: Bool = false
}

private final class MockIdentityService: IdentityServiceProtocol, @unchecked Sendable {
    let subject = CurrentValueSubject<Username?, Error>(nil)
    private(set) var subscribeCallCount = 0

    func subscribe(to _: AccountId) -> AnyPublisher<Username?, Error> {
        subscribeCallCount += 1
        return subject.eraseToAnyPublisher()
    }

    func username(for _: AccountId) -> AnyPublisher<Username?, Error> {
        Just<Username?>(nil).setFailureType(to: Error.self).eraseToAnyPublisher()
    }
}
