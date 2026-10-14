import Foundation
import Testing

@testable import polkadot_app

@Suite("W3sAmount")
struct W3sAmountTests {
    @Test("Accepts plain integers and one- or two-decimal-place amounts")
    func acceptsValidShapes() throws {
        let cases: [(input: String, normalized: String)] = [
            ("0", "0.00"),
            ("9", "9.00"),
            ("9.0", "9.00"),
            ("9.00", "9.00"),
            ("9.5", "9.50"),
            ("9.55", "9.55"),
            ("10000", "10000.00"),
            ("9999.99", "9999.99")
        ]
        for (raw, expected) in cases {
            let amount = try #require(W3sAmount.parse(raw, maxUnits: 10_000))
            #expect(amount.normalizedString == expected, "input \(raw)")
        }
    }

    @Test("Rejects shapes the spec calls out (3 dp, exponent, sign)")
    func rejectsForbiddenShapes() {
        // 3 decimal places.
        #expect(W3sAmount.parse("9.005") == nil)
        // Exponent.
        #expect(W3sAmount.parse("1e3") == nil)
        // Explicit sign.
        #expect(W3sAmount.parse("+9") == nil)
        #expect(W3sAmount.parse("-9") == nil)
        // Stray whitespace / leading dot / trailing dot.
        #expect(W3sAmount.parse(" 9") == nil)
        #expect(W3sAmount.parse("9 ") == nil)
        #expect(W3sAmount.parse(".9") == nil)
        #expect(W3sAmount.parse("9.") == nil)
        // Empty / non-numeric.
        #expect(W3sAmount.parse("") == nil)
        #expect(W3sAmount.parse("abc") == nil)
    }

    @Test("Enforces the maxUnits cap when supplied")
    func enforcesCap() {
        #expect(W3sAmount.parse("10000", maxUnits: 10_000) != nil)
        #expect(W3sAmount.parse("10000.01", maxUnits: 10_000) == nil)
        // No cap is applied when nil (DSFinV-K path).
        #expect(W3sAmount.parse("100000", maxUnits: nil) != nil)
    }

    @Test("fromValidatedDecimal canonicalises padding and rejects negatives")
    func fromValidatedDecimal() throws {
        let one = try #require(W3sAmount.fromValidatedDecimal(Decimal(1)))
        #expect(one.normalizedString == "1.00")

        let combined = try #require(W3sAmount.fromValidatedDecimal(Decimal(string: "9.00")! + Decimal(1)))
        #expect(combined.normalizedString == "10.00")

        // Negative values fail closed — the parser feeds us only validated positives,
        // but the helper is callable from elsewhere.
        #expect(W3sAmount.fromValidatedDecimal(Decimal(-1)) == nil)
    }
}
