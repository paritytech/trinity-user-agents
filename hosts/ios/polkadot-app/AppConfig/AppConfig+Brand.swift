import Foundation

/// Build-time brand identity, injected through Info.plist by Configs/brand.xcconfig.
extension AppConfig {
    enum Brand {
        static var displayName: String { string(.displayName) }
        static var appGroup: String { string(.appGroup) }
        static var bundleRoot: String { string(.bundleRoot) }
        static var deeplinkScheme: String { string(.deeplinkScheme) }
        static var deeplinkBase: String { string(.deeplinkBase) }
        static var shareRoot: String { string(.shareRoot) }
        static var cashSymbol: String { string(.cashSymbol) }
        static var fiatSymbol: String { string(.fiatSymbol) }
        static var termsURL: URL { url(.termsHost) }
        static var privacyURL: URL { url(.privacyHost) }
        static var contactEmail: String { string(.contactEmail) }
    }
}

private extension AppConfig.Brand {
    enum Key: String, CaseIterable {
        case displayName = "BrandDisplayName"
        case appGroup = "BrandAppGroup"
        case bundleRoot = "BrandBundleRoot"
        case deeplinkScheme = "BrandDeeplinkScheme"
        case deeplinkBase = "BrandDeeplinkBase"
        case shareRoot = "BrandShareRoot"
        case cashSymbol = "BrandCashSymbol"
        case fiatSymbol = "BrandFiatSymbol"
        case termsHost = "BrandTermsHost"
        case privacyHost = "BrandPrivacyHost"
        case contactEmail = "BrandContactEmail"
    }

    static func string(_ key: Key) -> String {
        guard
            let value = Bundle.main.object(forInfoDictionaryKey: key.rawValue) as? String,
            !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        else {
            fatalError(misconfiguration(key))
        }

        return value
    }

    /// Brand files store hosts without a scheme, because "//" starts a comment in an xcconfig.
    static func url(_ key: Key) -> URL {
        let host = string(key)

        guard let url = URL(string: "https://" + host), url.host() != nil else {
            fatalError(misconfiguration(key))
        }

        return url
    }

    static func misconfiguration(_ key: Key) -> String {
        """
        Info.plist key '\(key.rawValue)' is missing or empty. \
        Check Configs/brand.xcconfig against Configs/brand.template.xcconfig.
        """
    }
}
