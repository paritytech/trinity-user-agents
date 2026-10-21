import Foundation

#if targetEnvironment(simulator)
    import KeyDerivation
    import NovaCrypto

    /// Gives a new simulator the local state that the launch gates need. Then the
    /// truapi E2E launcher opens the dashboard without taps.
    ///
    /// The harness sets `TRUAPI_IOS_E2E_SEED_IDENTITY` to a lite username. The seed
    /// selects the theme, creates a random wallet and keeps the username. It does
    /// not register the username on chain. It marks the username as not claimed,
    /// thus the factory reset check does not compare it with the chain.
    /// The seed does not change a simulator that has a wallet or a username.
    enum TrUAPIE2EIdentitySeed {
        static func seedIfRequested(logger: LoggerProtocol = Logger.shared) {
            let environment = ProcessInfo.processInfo.environment
            guard let username = environment["TRUAPI_IOS_E2E_SEED_IDENTITY"]?
                .trimmingCharacters(in: .whitespacesAndNewlines),
                !username.isEmpty
            else {
                return
            }

            let themes = ThemeSelectionStorage()
            if !themes.hasSelectedTheme {
                themes.setSelected()
            }

            do {
                if try !RootEntropyManager.shared.hasRootEntropy() {
                    let mnemonic = try IRMnemonicCreator().randomMnemonic(.entropy128)
                    try RootEntropyManager.shared.createRootEntropy(mnemonic.entropy())
                    logger.debug("[e2e] created a random wallet")
                }
            } catch {
                logger.error("[e2e] failed to create the wallet: \(error)")
                return
            }

            let usernames = UsernameStorage()
            if !usernames.hasUsername {
                usernames.username = Username(value: username)
                usernames.usernameClaimed = false
                logger.debug("[e2e] stored the lite username \(username)")
            }
        }
    }
#endif
