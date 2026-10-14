import CryptoKit
import Foundation
import Testing

@testable import polkadot_app

@Suite("W3sPayDeeplinkService")
struct W3sPayDeeplinkServiceTests {
    /// 32-byte X25519 public key from a fresh keypair.
    private static let validKeyBytes = Curve25519.KeyAgreement.PrivateKey().publicKey.rawRepresentation
    private static var validKeyParam: String { validKeyBytes.base64URLEncodedString() }

    @MainActor
    private func makeService() -> (W3sPayDeeplinkService, MockLauncher) {
        let launcher = MockLauncher()
        return (W3sPayDeeplinkService(launcher: launcher), launcher)
    }

    @MainActor
    private func wait() async {
        // The service hops to MainActor via Task to invoke the launcher; yield once
        // so the captured launcher.lastLaunch is observable when we read it.
        await Task.yield()
        await Task.yield()
    }

    // MARK: - Host / path matching

    @Test("Returns false for wrong host")
    @MainActor
    func wrongHost() {
        let (service, launcher) = makeService()
        let url = URL(string: "polkadotapp://something-else/pay-w3s?id=abc&amount=1&key=\(Self.validKeyParam)")!
        #expect(service.handle(url: url) == false)
        #expect(launcher.lastLaunch == nil)
    }

    @Test("Returns false for wrong path")
    @MainActor
    func wrongPath() {
        let (service, launcher) = makeService()
        let url = URL(string: "polkadotapp://w3spay.dot/other?id=abc&amount=1&key=\(Self.validKeyParam)")!
        #expect(service.handle(url: url) == false)
        #expect(launcher.lastLaunch == nil)
    }

    // MARK: - Required query parameters

    @Test("Returns false when any of id / amount / key is missing")
    @MainActor
    func missingParams() {
        let (service, launcher) = makeService()
        let urls = [
            "polkadotapp://w3spay.dot/pay-w3s?amount=1&key=\(Self.validKeyParam)",
            "polkadotapp://w3spay.dot/pay-w3s?id=abc&key=\(Self.validKeyParam)",
            "polkadotapp://w3spay.dot/pay-w3s?id=abc&amount=1"
        ]
        for raw in urls {
            #expect(service.handle(url: URL(string: raw)!) == false, "url: \(raw)")
        }
        #expect(launcher.lastLaunch == nil)
    }

    // MARK: - id validation

    @Test("Rejects empty or non-alphanumeric id")
    @MainActor
    func rejectsInvalidId() {
        let (service, launcher) = makeService()
        // Empty.
        let empty = URL(string: "polkadotapp://w3spay.dot/pay-w3s?id=&amount=1&key=\(Self.validKeyParam)")!
        #expect(service.handle(url: empty) == false)
        // Hyphen — non-alphanumeric.
        let hyphen = URL(string: "polkadotapp://w3spay.dot/pay-w3s?id=abc-1&amount=1&key=\(Self.validKeyParam)")!
        #expect(service.handle(url: hyphen) == false)
        // Underscore — non-alphanumeric.
        let underscore = URL(string: "polkadotapp://w3spay.dot/pay-w3s?id=abc_1&amount=1&key=\(Self.validKeyParam)")!
        #expect(service.handle(url: underscore) == false)
        #expect(launcher.lastLaunch == nil)
    }

    // MARK: - amount validation

    @Test("Rejects amounts that violate the W3sAmount regex or the 10000 cap")
    @MainActor
    func rejectsInvalidAmounts() {
        let (service, launcher) = makeService()
        let amounts = ["9.005", "1e3", "+9", "abc", "10000.01"]
        for amount in amounts {
            let url = URL(string: "polkadotapp://w3spay.dot/pay-w3s?id=abc&amount=\(amount)&key=\(Self.validKeyParam)")!
            #expect(service.handle(url: url) == false, "amount: \(amount)")
        }
        #expect(launcher.lastLaunch == nil)
    }

    // MARK: - key validation

    @Test("Rejects merchant keys that are not 32 bytes of base64url-decoded data")
    @MainActor
    func rejectsInvalidKeys() {
        let (service, launcher) = makeService()
        // 31 bytes (one short).
        let shortKey = Data(repeating: 0x01, count: 31).base64URLEncodedString()
        // 33 bytes (one long — the legacy compressed-P256 length).
        let longKey = Data(repeating: 0x01, count: 33).base64URLEncodedString()
        // Standard-base64 "/" — strict decoder rejects.
        let badAlphabet = "A/" + String(repeating: "A", count: 42)
        for key in [shortKey, longKey, badAlphabet] {
            let url = URL(string: "polkadotapp://w3spay.dot/pay-w3s?id=abc&amount=1&key=\(key)")!
            #expect(service.handle(url: url) == false, "key: \(key.prefix(10))…")
        }
        #expect(launcher.lastLaunch == nil)
    }

    // MARK: - Happy path

    @Test("Forwards a well-formed URL to the launcher with derived topic + normalized amount")
    func happyPath() async throws {
        let (service, launcher) = await makeService()
        let url = URL(
            string: "polkadotapp://pay/cheque?id=abc123&amount=9.5&key=\(Self.validKeyParam)"
        )!
        #expect(service.handle(url: url) == true)
        await wait()

        let captured = try #require(await launcher.lastLaunch)
        #expect(captured.merchantKey == Self.validKeyBytes)
        #expect(captured.paymentId == "abc123")
        #expect(captured.amount.normalizedString == "9.50")
        // No merchant name on the deeplink path — recipient label is the id.
        #expect(captured.recipientLabel == "abc123")
        // Topic = blake2b256("pay-w3s:" || "abc123"). We re-derive it the same way
        // to confirm the service applied the documented prefix.
        let expectedTopic = try (Data("pay-w3s:".utf8) + Data("abc123".utf8)).blake2b32()
        #expect(captured.topic == expectedTopic)
    }
}

// MARK: - Test doubles

@MainActor
final class MockLauncher: W3sPayLaunching {
    struct Call: Equatable {
        let merchantKey: Data
        let topic: Data
        let paymentId: String
        let amount: W3sAmount
        let recipientLabel: String
    }

    private(set) var lastLaunch: Call?

    nonisolated init() {}

    func launch(
        merchantKey: Data,
        topic: Data,
        paymentId: String,
        amount: W3sAmount,
        recipientLabel: String
    ) {
        lastLaunch = Call(
            merchantKey: merchantKey,
            topic: topic,
            paymentId: paymentId,
            amount: amount,
            recipientLabel: recipientLabel
        )
    }
}
