import Foundation
import KeyDerivation
import Products

protocol AttachmentsSenderProviding {
    func getWallet() async throws -> WalletManaging
}

final class AttachmentsSenderProvider {
    let tldProvider: DotNsTldProviding
    let entropyManager: RootEntropyManaging

    init(
        entropyManager: RootEntropyManaging = RootEntropyManager.shared,
        tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared
    ) {
        self.entropyManager = entropyManager
        self.tldProvider = tldProvider
    }
}

extension AttachmentsSenderProvider: AttachmentsSenderProviding {
    func getWallet() async throws -> WalletManaging {
        let path = try ChatKeyPaths.hop(tld: tldProvider.currentTldOrError())
        return DynamicDerivedWallet(derivationPath: path, entropyManager: entropyManager)
    }
}
