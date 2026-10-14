#if TESTNET_FEATURE
    import KeyDerivation

    extension AppConfig {
        static let alice = DynamicDerivedWallet(
            mnemonic: "bottom drive obey lake curtain smoke basket hold race lonely fit walk",
            derivationPath: "//Alice"
        )

        #if UNSTABLE
            static var topupOrigin: DynamicDerivedWallet { alice }
        #else
            static let topupOrigin = DynamicDerivedWallet(
                mnemonic: "fluid truth dirt pulp rhythm decorate truck divert season tray cattle tumble"
            )
        #endif
    }
#endif
