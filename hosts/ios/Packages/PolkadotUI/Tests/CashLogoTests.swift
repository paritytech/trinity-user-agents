import Testing
import UIKit
@testable import PolkadotUI

struct CashLogoTests {
    private let font = UIFont.systemFont(ofSize: 17)

    @Test func capitalWordmarkOpensASentence() {
        let styled = CashLogo.styled("CASH Privacy Mode", symbol: "CASH", font: font)

        #expect(styled.string == "C Privacy Mode")
        #expect(fontName(of: styled, at: 0) == "CashLogo-Regular")
    }

    @Test func smallCapsWordmarkInsideASentence() {
        let styled = CashLogo.styled("Send you 5 CASH now", symbol: "CASH", font: font)

        #expect(styled.string == "Send you 5 c now")
        #expect(fontName(of: styled, at: 11) == "CashLogo-Regular")
    }

    @Test(arguments: ["Done. CASH arrives", "Done! CASH", "Done? CASH"])
    func capitalWordmarkAfterASentenceEnd(text: String) {
        let styled = CashLogo.styled(text, symbol: "CASH", font: font)

        #expect(styled.string.contains(" C"))
        #expect(!styled.string.contains(" c"))
    }

    @Test func eachTickerPicksItsOwnWordmark() {
        let styled = CashLogo.styled("CASH sent, 5 CASH left", symbol: "CASH", font: font)

        #expect(styled.string == "C sent, 5 c left")
    }

    @Test func onlyWholeWordsAreDrawn() {
        let styled = CashLogo.styled("CASHBACK and CASH", symbol: "CASH", font: font)

        #expect(styled.string == "CASHBACK and c")
    }

    @Test func otherSymbolsAreLeftAlone() {
        let styled = CashLogo.styled("DOT Privacy Mode", symbol: "DOT", font: font)

        #expect(styled.string == "DOT Privacy Mode")
        #expect(fontName(of: styled, at: 0) == font.fontName)
    }

    @Test func unitDrawsTheTickerAsOneGlyph() {
        #expect(CashLogo.unit("CASH", fallback: font).string == "c")
        #expect(CashLogo.unit("CASH", wordmark: .capital, fallback: font).string == "C")
        #expect(CashLogo.unit("DOT", fallback: font).string == "DOT")
    }

    private func fontName(of text: NSAttributedString, at location: Int) -> String? {
        (text.attribute(.font, at: location, effectiveRange: nil) as? UIFont)?.fontName
    }
}
