import Foundation
import Testing
@testable import Products

@Suite("TrustedRemoteProductPermissionRequester Tests")
struct TrustedRemoteProductPermissionRequesterTests {
    private let trusted = "peopl.dot"
    private let untrusted = "myapp.dot"

    private func makeSUT() -> (
        sut: TrustedRemoteProductPermissionRequester,
        wrapped: MockProductPermissionRequester
    ) {
        let wrapped = MockProductPermissionRequester()
        wrapped.decision = .deny
        let sut = TrustedRemoteProductPermissionRequester(
            isTrustedForRemoteAccess: { $0 == "peopl.dot" },
            wrapped: wrapped
        )
        return (sut, wrapped)
    }

    private static let remotePermissions: [ProductPermission] = [
        .networkAccess(domain: "example.com"),
        .webRtcAccess,
        .chainSubmitAccess,
        .preimageSubmitAccess,
        .statementSubmitAccess
    ]

    /// The core grants a first-party product every remote permission without
    /// prompting, and this is the path that decides the same question in the app.
    @Test("A trusted product gets remote access without reaching the user")
    func trustedProductSkipsRemotePrompts() async {
        for permission in Self.remotePermissions {
            let (sut, wrapped) = makeSUT()

            let decision = await sut.prompt(productId: trusted, permission: permission)

            #expect(decision == .allowAlways)
            #expect(wrapped.promptCalls.isEmpty, "\(permission) must not reach the user")
        }
    }

    /// The whole point of the narrower wrapper: trust for outbound access is not
    /// trust for the camera, another product's account, or the user's identity.
    @Test("A trusted product still prompts for everything that is not remote access")
    func trustedProductStillPromptsForTheRest() async {
        for permission: ProductPermission in [
            .deviceCapability(.camera),
            .accountAccess(targetProductId: "other.dot"),
            .balanceAccess,
            .userIdentityAccess
        ] {
            let (sut, wrapped) = makeSUT()

            let decision = await sut.prompt(productId: trusted, permission: permission)

            #expect(decision == .deny)
            #expect(wrapped.promptCalls.count == 1, "\(permission) must reach the user")
        }
    }

    @Test("An untrusted product prompts for remote access like any other")
    func untrustedProductPromptsForRemoteAccess() async {
        let (sut, wrapped) = makeSUT()

        let decision = await sut.prompt(
            productId: untrusted,
            permission: .networkAccess(domain: "example.com")
        )

        #expect(decision == .deny)
        #expect(wrapped.promptCalls.count == 1)
    }

    @Test("A batch of remote permissions is granted as one")
    func trustedProductSkipsRemoteBatches() async {
        let (sut, wrapped) = makeSUT()

        let decision = await sut.promptBatched(
            productId: trusted,
            permissions: Self.remotePermissions
        )

        #expect(decision == .allowAlways)
        #expect(wrapped.promptBatchedCalls.isEmpty)
    }

    /// One decision answers the whole batch, so a batch that mixes the two has
    /// to reach the user: granting it here would hand over the camera on the
    /// strength of a network grant.
    @Test("A batch mixing remote access with anything else still prompts")
    func aMixedBatchStillPrompts() async {
        let (sut, wrapped) = makeSUT()

        let decision = await sut.promptBatched(
            productId: trusted,
            permissions: [.networkAccess(domain: "example.com"), .deviceCapability(.camera)]
        )

        #expect(decision == .deny)
        #expect(wrapped.promptBatchedCalls.count == 1)
    }

    @Test("An empty batch asks for nothing and is not granted")
    func anEmptyBatchIsNotGranted() async {
        let (sut, wrapped) = makeSUT()

        let decision = await sut.promptBatched(productId: trusted, permissions: [])

        #expect(decision == .deny)
        #expect(wrapped.promptBatchedCalls.count == 1)
    }
}
