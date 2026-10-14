import UIKit
import Keystore_iOS
import NovaCrypto
import KeyDerivation

final class SecretPhraseMnemonicInteractor {
    weak var presenter: SecretPhraseMnemonicInteractorOutputProtocol?
    private let entropyManager: RootEntropyManaging
    private let mnemonicGenerator: IRMnemonicCreator
    private let logger: LoggerProtocol

    init(
        entropyManager: RootEntropyManaging,
        mnemonicGenerator: IRMnemonicCreator,
        logger: LoggerProtocol
    ) {
        self.entropyManager = entropyManager
        self.mnemonicGenerator = mnemonicGenerator
        self.logger = logger
    }
}

// MARK: - SecretPhraseMnemonicInteractorInputProtocol

extension SecretPhraseMnemonicInteractor: SecretPhraseMnemonicInteractorInputProtocol {
    func requestMnemonicData() {
        do {
            let data = try entropyManager.fetchRootEntropy()
            let mnemonic = try mnemonicGenerator.mnemonic(fromEntropy: data)
            MainActor.assumeIsolated {
                presenter?.didReceiveMnemonic(mnemonic)
            }
        } catch {
            logger.debug(error.localizedDescription)
        }
    }
}
