import Foundation
import Products
import Testing

@testable import polkadot_app

@Suite("Brand configuration")
struct BrandConfigTests {
    @Test("Every brand accessor resolves to a non-empty value")
    func everyAccessorResolves() {
        let values = [
            AppConfig.Brand.displayName,
            AppConfig.Brand.appGroup,
            AppConfig.Brand.deeplinkScheme,
            AppConfig.Brand.deeplinkBase,
            AppConfig.Brand.shareRoot,
            AppConfig.Brand.cashSymbol,
            AppConfig.Brand.fiatSymbol,
            AppConfig.Brand.contactEmail,
            AppConfig.Brand.termsURL.absoluteString,
            AppConfig.Brand.privacyURL.absoluteString
        ]

        for value in values {
            #expect(!value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
    }

    @Test("Deeplink scheme agrees with the registered URL type")
    func schemeAgreesWithRegisteredURLType() throws {
        let urlTypes = try #require(
            Bundle.main.object(forInfoDictionaryKey: "CFBundleURLTypes") as? [[String: Any]]
        )
        let schemes = try #require(urlTypes.first?["CFBundleURLSchemes"] as? [String])

        #expect(schemes.contains(AppConfig.DeepLink.scheme))
    }

    @Test("isKnownScheme accepts every brand flavor and rejects foreign schemes")
    func knownSchemeMatching() {
        #expect(AppConfig.DeepLink.isKnownScheme(AppConfig.DeepLink.scheme))
        #expect(AppConfig.DeepLink.isKnownScheme(DeeplinkTestSchemes.other))
        #expect(AppConfig.DeepLink.isKnownScheme(AppConfig.Brand.deeplinkBase))
        #expect(!AppConfig.DeepLink.isKnownScheme("tel"))
        #expect(!AppConfig.DeepLink.isKnownScheme("https"))
    }

    @Test("Active scheme is the brand base plus its configuration suffix")
    func activeSchemeDerivesFromBase() {
        #expect(AppConfig.DeepLink.scheme.hasPrefix(AppConfig.Brand.deeplinkBase))
    }

    @Test("App group and bundle identifier came from one expansion")
    func appGroupAgreesWithBundleIdentifier() throws {
        let bundleIdentifier = try #require(Bundle.main.bundleIdentifier)

        #expect(SharedContainerGroup.name == "group." + bundleIdentifier)
    }

    @Test("URL accessors parse as https hosts")
    func urlAccessorsParse() {
        #expect(AppConfig.Brand.termsURL.scheme == "https")
        #expect(AppConfig.Brand.termsURL.host() != nil)
        #expect(AppConfig.Brand.privacyURL.scheme == "https")
        #expect(AppConfig.Brand.privacyURL.host() != nil)
        #expect(AppConfig.Brand.contactEmail.contains("@"))
    }

    /// EXPECTED TO FAIL under any non-polkadot brand. ProductHost.shareRootDomains is the
    /// inbound universal-link allow-list and is hardcoded to the polkadot roots, so iOS hands
    /// the app a link that WebDeeplinkNormalizer then rejects. The failure is the intended
    /// signal for that known blocker, not a regression in this suite.
    @Test("Share root is matchable inbound")
    func shareRootIsMatchableInbound() {
        #expect(ProductHost.shareHosts.contains(AppConfig.Brand.shareRoot))
    }
}
