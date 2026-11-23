import Foundation
@testable import polkadot_app
import Testing
import TrUAPIHost

/// The keypad's edits to the typed dollar amount.
struct FundingKeypadTests {
    @Test
    func theKeypadEditsADollarAmountOfAtMostTwoDecimals() {
        let apply = { (key: FundingKeypad.Key, text: String) in FundingKeypad.apply(key, to: text) }

        #expect(apply(.digit("5"), "") == "5")
        #expect(apply(.digit("5"), "0") == "5")
        #expect(apply(.point, "") == "0.")
        #expect(apply(.point, "12.") == "12.")
        #expect(apply(.digit("9"), "12.3") == "12.39")
        #expect(apply(.digit("9"), "12.39") == "12.39")
        #expect(apply(.digit("1"), "1234567") == "1234567")
        #expect(FundingKeypad.apply(.digit("1"), to: "12", maximumIntegerDigits: 2) == "12")
        #expect(apply(.delete, "12.3") == "12.")
        #expect(apply(.delete, "") == "")
    }
}
