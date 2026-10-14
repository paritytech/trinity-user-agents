import Foundation
import Testing

@testable import polkadot_app

@Suite("Payment asset remote config")
struct PaymentAssetConfigTests {
    @Test("Parses the shared object shape")
    func parsesFullObject() throws {
        let config = try #require(PaymentAssetConfig(json: [
            "symbol": "CASH",
            "iconSquareUrl": "https://cdn.example.com/cash/square.svg",
            "iconWideUrl": "https://cdn.example.com/cash/wide.svg"
        ]))

        #expect(config.symbol == "CASH")
        #expect(config.squareIconURL?.absoluteString == "https://cdn.example.com/cash/square.svg")
        #expect(config.wideIconURL?.absoluteString == "https://cdn.example.com/cash/wide.svg")
    }

    @Test("Every field is optional")
    func partialObject() throws {
        let config = try #require(PaymentAssetConfig(json: ["symbol": " USD "]))

        #expect(config.symbol == "USD")
        #expect(config.squareIconURL == nil)
        #expect(config.wideIconURL == nil)
    }

    @Test("An object with nothing usable reads as not published")
    func emptyObjectIsNil() {
        #expect(PaymentAssetConfig(json: [:]) == nil)
        #expect(PaymentAssetConfig(json: ["symbol": "  ", "iconSquareUrl": ""]) == nil)
        #expect(PaymentAssetConfig(json: ["unrelated": "value"]) == nil)
    }

    @Test("Only absolute web URLs are accepted as logos")
    func rejectsNonWebURLs() throws {
        let config = try #require(PaymentAssetConfig(json: [
            "symbol": "CASH",
            "iconSquareUrl": "square.svg",
            "iconWideUrl": "file:///etc/passwd"
        ]))

        #expect(config.squareIconURL == nil)
        #expect(config.wideIconURL == nil)
    }
}
