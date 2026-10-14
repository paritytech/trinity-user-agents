import Foundation
import KeyDerivation

@testable import polkadot_app

struct MockAttachmentsSenderProvider: AttachmentsSenderProviding {
    func getWallet(for _: Chat.Id) async throws -> WalletManaging {
        try MockWalletManager.mockedWallet()
    }
}
