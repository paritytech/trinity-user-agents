import Foundation
import SubstrateSdk
import KeyDerivation

public protocol OriginPersonProviding {
    func pickPersonOrigin() async throws -> PersonOrigin
    func pickPersonOrigins() async throws -> [PersonOrigin]
}

public enum OriginPersonProviderError: Error {
    case noPersonsExist
}

/// What to do when neither person is on chain at the time of the read.
public enum NoPersonStrategy: Sendable {
    /// Fail right away with ``OriginPersonProviderError/noPersonsExist``.
    case noPersonError
    /// The lite person may still be registering — its entry lands a few blocks after onboarding — so
    /// watch its status for up to the delay before failing.
    case waitLight(Duration)

    public static let `default`: NoPersonStrategy = .waitLight(.seconds(20))
}

// Returns all origins the person is a member of, full first. Checks ring status of both lite and full person.
public final class OriginPersonProvider {
    let liteVrfManager: BandersnatchKeyManaging
    let liteCollectionId: MembersPallet.CollectionIdentifier
    let fullVrfManager: BandersnatchKeyManaging
    let fullCollectionId: MembersPallet.CollectionIdentifier
    let memberStatusChecker: MembershipStatusChecking
    let liteStatusWaiter: MembershipStatusWaiting
    let noPersonStrategy: NoPersonStrategy

    public init(
        liteVrfManager: BandersnatchKeyManaging,
        liteCollectionId: MembersPallet.CollectionIdentifier,
        fullVrfManager: BandersnatchKeyManaging,
        fullCollectionId: MembersPallet.CollectionIdentifier,
        memberStatusChecker: MembershipStatusChecking,
        liteStatusWaiter: MembershipStatusWaiting,
        noPersonStrategy: NoPersonStrategy = .default
    ) {
        self.liteVrfManager = liteVrfManager
        self.liteCollectionId = liteCollectionId
        self.fullVrfManager = fullVrfManager
        self.fullCollectionId = fullCollectionId
        self.memberStatusChecker = memberStatusChecker
        self.liteStatusWaiter = liteStatusWaiter
        self.noPersonStrategy = noPersonStrategy
    }

    /// Reads and waits over one chain connection.
    public convenience init(
        liteVrfManager: BandersnatchKeyManaging,
        liteCollectionId: MembersPallet.CollectionIdentifier,
        fullVrfManager: BandersnatchKeyManaging,
        fullCollectionId: MembersPallet.CollectionIdentifier,
        connection: JSONRPCEngine,
        runtimeCodingService: RuntimeCodingServiceProtocol,
        noPersonStrategy: NoPersonStrategy = .default
    ) {
        let checker = MembershipStatusChecker(connection: connection, runtimeCodingService: runtimeCodingService)
        self.init(
            liteVrfManager: liteVrfManager,
            liteCollectionId: liteCollectionId,
            fullVrfManager: fullVrfManager,
            fullCollectionId: fullCollectionId,
            memberStatusChecker: checker,
            liteStatusWaiter: MembershipStatusWaiter(
                connection: connection,
                runtimeCodingService: runtimeCodingService,
                statusChecker: checker
            ),
            noPersonStrategy: noPersonStrategy
        )
    }
}

extension OriginPersonProvider: OriginPersonProviding {
    public func pickPersonOrigins() async throws -> [PersonOrigin] {
        let fullMemberKey = try fullVrfManager.getMemberKey()
        let liteMemberKey = try liteVrfManager.getMemberKey()

        let statuses = try await memberStatusChecker.checkStatuses(
            of: [
                .init(memberKey: fullMemberKey, collection: fullCollectionId),
                .init(memberKey: liteMemberKey, collection: liteCollectionId),
            ],
            blockHash: nil
        )

        var origins: [PersonOrigin] = []

        if let fullRingIndex = statuses[fullMemberKey] {
            origins.append(.full(fullRingIndex, fullVrfManager))
        }

        if let liteRingIndex = statuses[liteMemberKey] {
            origins.append(.lite(liteRingIndex, liteVrfManager))
        }

        guard origins.isEmpty else {
            return origins
        }

        return try await [originAfterNoPerson(liteMemberKey: liteMemberKey)]
    }

    public func pickPersonOrigin() async throws -> PersonOrigin {
        guard let origin = try await pickPersonOrigins().first else {
            throw OriginPersonProviderError.noPersonsExist
        }
        return origin
    }
}

private extension OriginPersonProvider {
    func originAfterNoPerson(liteMemberKey: MembersPallet.RingMember) async throws -> PersonOrigin {
        switch noPersonStrategy {
        case .noPersonError:
            throw OriginPersonProviderError.noPersonsExist
        case let .waitLight(delay):
            let liteInput = MembershipStatusInput(memberKey: liteMemberKey, collection: liteCollectionId)
            guard let ringIndex = try await liteStatusWaiter.waitForStatus(of: liteInput, timeout: delay) else {
                throw OriginPersonProviderError.noPersonsExist
            }
            return .lite(ringIndex, liteVrfManager)
        }
    }
}
