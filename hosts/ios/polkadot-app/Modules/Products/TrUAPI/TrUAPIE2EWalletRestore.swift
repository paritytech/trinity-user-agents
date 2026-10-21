import Foundation

#if targetEnvironment(simulator)
    import KeyDerivation
    import NovaCrypto

    /// Restores the wallet of an account that has a registered username, from a mnemonic that the truapi E2E
    /// harness gives. It is the step of the restore screen after the user types the words.
    ///
    /// The harness sets `TRUAPI_IOS_E2E_RESTORE_WALLET=1` and writes the mnemonic to `tmp/truapi-e2e/seed`. The
    /// hook reads the file, removes it, and creates the wallet. Then the username gate of the app finds the
    /// username of the account on chain, as after a restore through the screens. The hook changes no chain state
    /// and keeps no username of its own.
    ///
    /// The hook does not change a simulator that has the same wallet. It refuses a different wallet.
    enum TrUAPIE2EWalletRestore {
        static let seedFile = FileManager.default.temporaryDirectory
            .appendingPathComponent("truapi-e2e/seed")

        static func restoreIfRequested(logger: LoggerProtocol = Logger.shared) {
            guard ProcessInfo.processInfo.environment["TRUAPI_IOS_E2E_RESTORE_WALLET"] == "1" else { return }

            let themes = ThemeSelectionStorage()
            if !themes.hasSelectedTheme {
                themes.setSelected()
            }

            do {
                try restoreWallet(logger: logger)
            } catch {
                // Not the description of the error, which can quote a word of the mnemonic.
                let nsError = error as NSError
                logger.error("[e2e] wallet restore failed: \(type(of: error)) \(nsError.domain) \(nsError.code)")
            }
        }

        private static func restoreWallet(logger: LoggerProtocol) throws {
            guard FileManager.default.fileExists(atPath: seedFile.path) else {
                logger.debug("[e2e] no seed file, the wallet stays as it is")
                return
            }

            let phrase: String
            do {
                defer { try? FileManager.default.removeItem(at: seedFile) }
                phrase = try String(contentsOf: seedFile, encoding: .utf8)
            }

            let mnemonic = try IRMnemonicCreator().mnemonic(fromList: MnemonicTextNormalizer().process(text: phrase))
            let entropyManager = RootEntropyManager.shared

            if try entropyManager.hasRootEntropy() {
                guard try entropyManager.fetchRootEntropy() == mnemonic.entropy() else {
                    throw TrUAPIE2EWalletRestoreError.otherWallet
                }
                logger.debug("[e2e] the wallet of the seed is already set up")
                return
            }

            let walletSetupManager = WalletSetupManager(
                mnemonicGenerator: IRMnemonicCreator(),
                mnemonicBackupHelper: MnemonicBackupHelper(),
                entropyManager: entropyManager,
                logger: logger
            )
            try walletSetupManager.createWallets(with: AccountCreateMetadata(mnemonic: mnemonic))
            logger.debug("[e2e] restored the wallet from the seed")
        }
    }

    enum TrUAPIE2EWalletRestoreError: Error {
        /// The simulator has a different wallet. Reinstall the app to restore another one.
        case otherWallet
    }
#endif
