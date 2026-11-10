import Foundation
import KeyDerivation

@testable import polkadot_app

struct MockAttachmentsSenderProvider: AttachmentsSenderProviding {
    func getWallet() async throws -> WalletManaging {
        try MockWalletManager.mockedWallet()
    }
}
