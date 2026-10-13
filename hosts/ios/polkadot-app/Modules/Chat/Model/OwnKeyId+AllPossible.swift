import Foundation
import Products

extension Chat.Contact.Own {
    static func main(tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared) throws -> Chat.Contact.Own {
        try Chat.Contact.Own(
            signKeyId: WalletDerivationPath.main(for: tldProvider.currentTldOrError()),
            encryptionKeyId: ChatEncryptionDomain.mainChat.rawValue
        )
    }

    static func sso(tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared) throws -> Chat.Contact.Own {
        try Chat.Contact.Own(
            signKeyId: WalletDerivationPath.main(for: tldProvider.currentTldOrError()),
            encryptionKeyId: ChatEncryptionDomain.sso.rawValue
        )
    }

    static func allPossibleIds(
        tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared
    ) throws -> Set<Chat.Contact.Own> {
        try [
            main(tldProvider: tldProvider),
            sso(tldProvider: tldProvider)
        ]
    }
}
