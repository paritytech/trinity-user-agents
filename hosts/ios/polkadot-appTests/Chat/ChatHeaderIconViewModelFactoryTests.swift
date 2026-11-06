import PolkadotUI
import Testing
import TrUAPIHost
@testable import polkadot_app

private let productId = try! ProductId("dim2.paseo")

/// The header loads its icon again on every chat update, so it keeps the one it built for the peer.
@Suite("Chat header icon")
struct ChatHeaderIconViewModelFactoryTests {
    @Test
    func aPublicProfileIsRefreshedOncePerPeer() {
        let publicProfiles = RecordingPublicProfileAvatarViewModelFactory()
        let headerIcons = ChatHeaderIconViewModelFactory(
            productFactory: RecordingProductIconViewModelFactory(),
            publicProfileFactory: publicProfiles
        )

        _ = headerIcons.viewModel(for: .publicProfile(username: "alice.01"))
        _ = headerIcons.viewModel(for: .publicProfile(username: "alice.01"))
        _ = headerIcons.viewModel(for: .publicProfile(username: "bob.02"))

        #expect(publicProfiles.requests == [
            .init(username: "alice.01", refresh: true),
            .init(username: "bob.02", refresh: true)
        ])
    }

    /// A product has no profile to reread, so its icon loads by domain as in the chat list.
    @Test
    func aProductIconLoadsByDomain() {
        let products = RecordingProductIconViewModelFactory()
        let publicProfiles = RecordingPublicProfileAvatarViewModelFactory()
        let headerIcons = ChatHeaderIconViewModelFactory(productFactory: products, publicProfileFactory: publicProfiles)

        _ = headerIcons.viewModel(for: .product(domain: productId))

        #expect((products.requestedDomains, publicProfiles.requests) == ([productId], []))
    }
}

private final class RecordingPublicProfileAvatarViewModelFactory: PublicProfileAvatarViewModelMaking {
    struct Request: Equatable {
        let username: String
        let refresh: Bool
    }

    private(set) var requests: [Request] = []

    func createViewModel(for username: String, refresh: Bool) -> any ImageViewModelProtocol {
        requests.append(Request(username: username, refresh: refresh))
        return StaticImageViewModel(image: nil)
    }
}

private final class RecordingProductIconViewModelFactory: ProductIconViewModelMaking {
    private(set) var requestedDomains: [ProductId] = []

    func createViewModel(for domain: ProductId) -> any ImageViewModelProtocol {
        requestedDomains.append(domain)
        return StaticImageViewModel(image: nil)
    }
}
