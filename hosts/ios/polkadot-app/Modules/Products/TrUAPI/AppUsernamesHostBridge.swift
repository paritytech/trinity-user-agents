import Foundation
import Operation_iOS
import Products
import SubstrateSdk
import TrUAPIHost
import UIKit
import UIKitExt

/// The username directory that products use: the search of the chat contact search, and an add that asks the user
/// and then opens the chat request screen of the app. The product learns usernames only, never an account.
/// The core already checked the tap of a Worker.
final class AppUsernamesHostBridge: UsernamesHostBridge, @unchecked Sendable {
    /// The most usernames that one search gives.
    static let searchLimit = 20

    private let directory: RemoteContactOperationMaking
    private let contactAccounts: @Sendable () async throws -> Set<AccountId>
    private let screen: ProductAddContactScreen

    init(
        directory: RemoteContactOperationMaking,
        contactAccounts: @escaping @Sendable () async throws -> Set<AccountId>,
        screen: ProductAddContactScreen
    ) {
        self.directory = directory
        self.contactAccounts = contactAccounts
        self.screen = screen
    }

    func searchUsernames(productId _: String, prefix: String) async throws -> [String] {
        var seen = Set<String>()
        let usernames = try await directory.search(by: prefix)
            .map(\.username)
            .filter { seen.insert($0.lowercased()).inserted }
        return Array(usernames.prefix(Self.searchLimit))
    }

    func addContact(
        productId: String,
        executionKind _: ProductExecutionKind,
        username: String
    ) async throws -> HostContactAdd {
        let wanted = Self.plain(username)
        let found = try await directory.search(by: wanted).first { Self.plain($0.username) == wanted }
        guard let found else { return .notFound }
        if try await contactAccounts().contains(found.accountId) { return .alreadyContact }

        let answer = await screen.confirmAdd(product: productId, username: found.username)
        switch answer {
        case .none:
            return .notVisible
        case false?:
            return .dismissed
        case true?:
            try await screen.openRequest(to: found)
            return .added
        }
    }

    private static func plain(_ username: String) -> String {
        String(username.drop { $0 == "@" }).lowercased()
    }
}

extension AppUsernamesHostBridge {
    /// The bridge of the app: the remote contact search, the contacts of the chat store, the products router for
    /// the question and the chat tab for the request.
    convenience init(routerFacade: ProductRoutersFacadeProtocol, repositoryFactory: ChatContactRepositoryMaking) {
        self.init(
            directory: RemoteContactOperationFactory(),
            contactAccounts: {
                let contacts = try await repositoryFactory
                    .createRepository(forFilter: NSPredicate.isContact())
                    .fetchAllOperation(with: RepositoryFetchOptions())
                    .asyncExecute()
                return Set(contacts.map(\.accountId))
            },
            screen: AppProductAddContactScreen(routerFacade: routerFacade, navigator: ModuleNavigator())
        )
    }
}

/// The screens of an add of a contact.
protocol ProductAddContactScreen: Sendable {
    /// Asks the user. Nil when no screen can show the question.
    func confirmAdd(product: String, username: String) async -> Bool?

    /// Opens the chat request screen for the person, as the contact search of the app does.
    func openRequest(to contact: Chat.RemoteContact) async throws
}

final class AppProductAddContactScreen: ProductAddContactScreen, @unchecked Sendable {
    private let routerFacade: ProductRoutersFacadeProtocol
    private let navigator: ModuleNavigating

    init(routerFacade: ProductRoutersFacadeProtocol, navigator: ModuleNavigating) {
        self.routerFacade = routerFacade
        self.navigator = navigator
    }

    func confirmAdd(product: String, username: String) async -> Bool? {
        await withCheckedContinuation { continuation in
            Task { @MainActor [routerFacade] in
                let alert = ProductAddContactAlert(product: product, username: username) { accepted in
                    continuation.resume(returning: accepted)
                }
                if !routerFacade.productsRouter.present(view: alert) {
                    continuation.resume(returning: nil)
                }
            }
        }
    }

    func openRequest(to contact: Chat.RemoteContact) async throws {
        let request = try ChatOpenModel.NewRequest(remoteContact: contact, ownKeyId: Chat.Contact.Own.main())
        await navigator.openChat(.newRequest(request))
    }
}

/// The question of a product that asks to add a contact.
final class ProductAddContactAlert: UIAlertController, ControllerBackedProtocol {
    convenience init(product: String, username: String, onAnswer: @escaping (Bool) -> Void) {
        self.init(
            title: String(localized: .ChatExtension.productAddContactTitle),
            message: String(localized: .ChatExtension.productAddContactMessage(product: product, username: username)),
            preferredStyle: .alert
        )
        addAction(UIAlertAction(title: String(localized: .Common.cancel), style: .cancel) { _ in onAnswer(false) })
        let confirm = String(localized: .ChatExtension.productAddContactConfirm)
        addAction(UIAlertAction(title: confirm, style: .default) { _ in onAnswer(true) })

        #if targetEnvironment(simulator)
            TrUAPIE2EPendingNotice.onAnswer = { [weak self] accepted in
                guard let self else { return onAnswer(accepted) }
                dismiss(animated: false) { onAnswer(accepted) }
            }
        #endif
    }
}
