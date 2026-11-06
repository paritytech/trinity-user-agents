import Foundation
@testable import PolkadotUI
import Testing
import UIKit
@testable import polkadot_app

@MainActor
struct SearchContactPresenterAvatarTests {
    @Test(.timeLimit(.minutes(1)))
    func eachSearchSectionResolvesTheUsernameOfItsContact() async throws {
        let suffix = UUID().uuidString
        let usernames = ["recent-\(suffix)", "stored-\(suffix)", "remote-\(suffix)"]
        let resolver = MockPublicProfileAvatarResolver(result: .failure(.Unavailable(reason: "no profile avatar")))
        let presenter = makePresenter(resolver: resolver)
        let view = MockSearchContactView()
        presenter.view = view
        let sections = AccountSearchSections(
            recent: [makeLocalRow(username: usernames[0], accountByte: 1)],
            contacts: [makeLocalRow(username: usernames[1], accountByte: 2)],
            global: .loaded([try makeRemoteRow(username: usernames[2])])
        )

        presenter.didReceive(searchState: .started, for: "query")
        presenter.didReceive(searchState: .result(.sections(sections)), for: "query")
        let configurations = try #require(view.viewModel).sections.flatMap { $0.rows.map(\.configuration) }

        #expect(configurations.map(\.userName) == usernames)
        for configuration in configurations {
            let icon = try #require(configuration.icon)
            let imageView = UIImageView()
            await withCheckedContinuation { continuation in
                icon.loadImage(on: imageView, settings: .originalImage, animated: false) { _ in
                    continuation.resume()
                }
            }
        }

        #expect(await resolver.requestedUsernames == usernames)
    }

    @Test
    func anUnnamedContactDoesNotGetAnAvatarLoader() throws {
        let presenter = makePresenter(
            resolver: MockPublicProfileAvatarResolver(result: .failure(.Unavailable(reason: "no profile avatar")))
        )
        let view = MockSearchContactView()
        presenter.view = view
        let sections = AccountSearchSections<ContactSearchPayload, ContactSearchPayload>(
            recent: [],
            contacts: [makeLocalRow(username: "", accountByte: 1)]
        )

        presenter.didReceive(searchState: .result(.sections(sections)), for: "")

        let configuration = try #require(view.viewModel?.sections.first?.rows.first?.configuration)
        #expect(configuration.icon == nil)
    }
}

private extension SearchContactPresenterAvatarTests {
    func makePresenter(resolver: MockPublicProfileAvatarResolver) -> SearchContactPresenter {
        SearchContactPresenter(
            interactor: StubSearchContactInteractor(),
            wireframe: SearchContactWireframe(),
            publicProfileAvatarViewModelFactory: PublicProfileAvatarViewModelFactory(
                resolveProfile: { username, force in
                    await resolver.resolve(username: username, force: force)
                },
                loadAvatar: { avatar in
                    try await resolver.loadAvatar(avatar)
                }
            )
        )
    }

    func makeLocalRow(username: String, accountByte: UInt8) -> SearchRow<ContactSearchPayload> {
        let contact = Chat.Contact(
            accountId: Data(repeating: accountByte, count: 32),
            username: username,
            publicKey: Data(repeating: 2, count: 32),
            ownKeyId: .init(signKeyId: "sign", encryptionKeyId: "encryption"),
            imageData: nil,
            isBlocked: false,
            devices: [],
            pendingDevicesFanOut: false
        )
        return SearchRow(accountId: contact.accountId, username: nil, matchTerms: [], payload: .local(contact))
    }

    func makeRemoteRow(username: String) throws -> SearchRow<ContactSearchPayload> {
        let contact = Chat.RemoteContact(
            accountId: Data(repeating: 3, count: 32),
            username: username,
            chatPublicKey: try Chat.PublicKey(rawData: Data(repeating: 4, count: 32)),
            imageData: nil
        )
        return SearchRow(accountId: contact.accountId, username: nil, matchTerms: [], payload: .remote(contact))
    }
}
