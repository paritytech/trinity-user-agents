import AsyncExtensions
import Foundation
import Testing
@testable import Individuality

struct MembershipStatusWaiterTests {
    private static let collection = Data(repeating: 1, count: 32)
    private static let memberKey = Data(repeating: 2, count: 33)
    private static let keysPerPage = 4
    private static let bound: Duration = .seconds(10)

    private let input = MembershipStatusInput(memberKey: memberKey, collection: collection)
    private let subscriber = StubMembershipSubscriber(keysPerPage: keysPerPage)

    private var waiter: MembershipStatusWaiter { MembershipStatusWaiter(subscriber: subscriber) }

    /// Page 1, position 1 is the sixth key of the ring, so the root covers it only from `included == 6`.
    private static let inclusion = MembersPallet.RingPosition.Included(ringIndex: 2, ringPage: 1, ringPosition: 1)

    @Test("an included member is returned once the ring root covers its key")
    func rootCatchesUp() async throws {
        subscriber.send(.included(Self.inclusion))
        subscriber.send(MembersPallet.RingKeysStatus(total: 6, included: 5))
        subscriber.send(MembersPallet.RingKeysStatus(total: 6, included: 6))

        let ringIndex = try await waiter.waitForStatus(of: input, timeout: Self.bound)

        #expect(ringIndex == 2)
        #expect(subscriber.statusRequests == [.init(collection: Self.collection, ringIndex: 2)])
    }

    @Test("a ring status that never covers the key is not an inclusion")
    func rootNeverCatchesUp() async throws {
        subscriber.send(.included(Self.inclusion))
        subscriber.send(MembersPallet.RingKeysStatus(total: 6, included: 5))
        subscriber.endStatuses()

        await #expect(throws: MembershipStatusWaiterError.subscriptionEnded) {
            try await waiter.waitForStatus(of: input, timeout: Self.bound)
        }
    }

    @Test("a member still onboarding is waited for until its entry is included")
    func onboardingThenIncluded() async throws {
        subscriber.send(.onboarding(.init(queuePage: 0, queuedAt: 1)))
        subscriber.send(.included(Self.inclusion))
        subscriber.send(MembersPallet.RingKeysStatus(total: 6, included: 6))

        let ringIndex = try await waiter.waitForStatus(of: input, timeout: Self.bound)

        #expect(ringIndex == 2)
    }

    @Test("a members stream that ends before inclusion fails")
    func membersStreamEnds() async throws {
        subscriber.send(.onboarding(.init(queuePage: 0, queuedAt: 1)))
        subscriber.endPositions()

        await #expect(throws: MembershipStatusWaiterError.subscriptionEnded) {
            try await waiter.waitForStatus(of: input, timeout: Self.bound)
        }
    }
}

private final class StubMembershipSubscriber: MembershipSubscribing, @unchecked Sendable {
    struct StatusRequest: Equatable {
        let collection: MembersPallet.CollectionIdentifier
        let ringIndex: MembersPallet.RingIndex
    }

    private let keysPerPage: Int
    private let positions = AsyncStream<MembersPallet.RingPosition?>.makeStream()
    private let statuses = AsyncStream<MembersPallet.RingKeysStatus?>.makeStream()
    private(set) var statusRequests: [StatusRequest] = []

    init(keysPerPage: Int) {
        self.keysPerPage = keysPerPage
    }

    func send(_ position: MembersPallet.RingPosition?) {
        positions.continuation.yield(position)
    }

    func send(_ status: MembersPallet.RingKeysStatus?) {
        statuses.continuation.yield(status)
    }

    func endPositions() {
        positions.continuation.finish()
    }

    func endStatuses() {
        statuses.continuation.finish()
    }

    func memberPositions(of _: MembershipStatusInput) -> AnyAsyncSequence<MembersPallet.RingPosition?> {
        positions.stream.eraseToAnyAsyncSequence()
    }

    func ringKeysStatuses(
        collection: MembersPallet.CollectionIdentifier,
        ringIndex: MembersPallet.RingIndex
    ) -> AnyAsyncSequence<MembersPallet.RingKeysStatus?> {
        statusRequests.append(StatusRequest(collection: collection, ringIndex: ringIndex))
        return statuses.stream.eraseToAnyAsyncSequence()
    }

    func ringKeysPageSize() async throws -> Int {
        keysPerPage
    }
}
