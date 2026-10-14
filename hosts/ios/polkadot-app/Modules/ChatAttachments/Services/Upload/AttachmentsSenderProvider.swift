import Foundation
import SubstrateSdk
import Operation_iOS
import KeyDerivation
import Products
import StructuredConcurrency

protocol AttachmentsSenderProviding {
    func getWallet(for chatId: Chat.Id) async throws -> WalletManaging
}

final class AttachmentsSenderProvider {
    let contactsStorageService: ContactsLocalStorageServicing
    let tldProvider: DotNsTldProviding

    init(
        contactsStorageService: ContactsLocalStorageServicing = ContactsLocalStorageService(),
        tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared
    ) {
        self.contactsStorageService = contactsStorageService
        self.tldProvider = tldProvider
    }
}

extension AttachmentsSenderProvider: AttachmentsSenderProviding {
    func getWallet(for chatId: Chat.Id) async throws -> WalletManaging {
        let mainSignKeyId = try WalletDerivationPath.main(for: tldProvider.currentTldOrError())

        guard let accountId = chatId.accountId else {
            return DynamicDerivedWallet(derivationPath: mainSignKeyId)
        }

        let contact = try await contactsStorageService
            .getContact(by: accountId)
            .asyncExecute()

        let signKeyId = contact?.ownKeyId.signKeyId ?? mainSignKeyId

        return DynamicDerivedWallet(derivationPath: signKeyId)
    }
}
