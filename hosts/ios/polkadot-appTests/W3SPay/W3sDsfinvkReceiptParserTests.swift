import Foundation
import Testing

@testable import polkadot_app

@Suite("W3sDsfinvkReceiptParser")
struct W3sDsfinvkReceiptParserTests {
    private let parser = W3sDsfinvkReceiptParser()

    @Test("Parses the first real-world fixture from the spec")
    func parsesFirstFixture() throws {
        let raw = "V0;1342061307;Kassenbeleg-V1;Beleg^9.00_0.00_0.00_0.00_0.00^9.00:Bar;"
            + "8041;17011;2026-05-21T21:40:56.000Z;2026-05-21T21:41:30.000Z;ecdsa-plain-SHA256;"
            + "unixTime;D5CdNgSCwlSisYXjQoZnIxAM9nRdb91F8l6uIdR+oKWi7/kTszRK/xRLBNgcGhc6L1ChYQkt"
            + "JJFmzKFt8mTn/w==;BJlf238fEMG/ycfzOUBpIHa8OZNMXFMZx9ug42Vs6F0zOx42io2pnoWnRvoNeITA"
            + "Y1J4+2ePsszO3CeJrgfLWb8="
        let receipt = try #require(parser.tryParse(raw))
        #expect(receipt.serial == "1342061307")
        #expect(receipt.transactionNumber == "8041")
        #expect(receipt.amount.normalizedString == "9.00")
        #expect(receipt.paymentId == "1342061307/8041")
    }

    @Test("Parses the second real-world fixture (serial with spaces+hyphens, ERS prefix)")
    func parsesSecondFixture() throws {
        let raw = "V0;ERS aacbe40e-3aa6-48a1-b8e6-3c8abbd7ebd5;Kassenbeleg-V1;"
            + "Beleg^0.00_8.20_0.00_0.00_1.80^10.00:Bar;11;138;2023-12-13T16:01:44.000Z;"
            + "2023-12-13T16:01:56.000Z;ecdsa-plain-SHA256;unixTime;"
            + "7nTRzYZ6Pjhnv5l8Qo/Gs9cQ2KvmCMMK1/rQ0hoPtjkp2tPr3yuWPvH9rkOkCFuAI79k/VxvkyxwSQyWjRc7iA==;"
            + "BCC8Xaw0n2bnmcsOpLwgYhlUEw/aOvJPFMy2WOFaabktCrxep80VY7Y8KdrjIAx+9ta7wfMO03k4nwN11ZnNKm4="
        let receipt = try #require(parser.tryParse(raw))
        #expect(receipt.serial == "ERS aacbe40e-3aa6-48a1-b8e6-3c8abbd7ebd5")
        #expect(receipt.transactionNumber == "11")
        #expect(receipt.amount.normalizedString == "10.00")
        #expect(receipt.paymentId == "ERS aacbe40e-3aa6-48a1-b8e6-3c8abbd7ebd5/11")
    }

    @Test("Sums multiple payment entries split by underscores")
    func sumsPaymentEntries() throws {
        // payments: 5.00:Bar + 3.50:Karte:EUR = 8.50
        let raw = "V0;CASH-1;Kassenbeleg-V1;Beleg^0.00^5.00:Bar_3.50:Karte:EUR;42;1;a;b;c;d;e;f"
        let receipt = try #require(parser.tryParse(raw))
        #expect(receipt.amount.normalizedString == "8.50")
    }

    @Test("Rejects QR strings without the V0; prefix")
    func rejectsNonDsfinvk() {
        #expect(parser.tryParse("polkadotapp://w3spay.dot/pay-w3s?id=x&amount=1&key=k") == nil)
        #expect(parser.tryParse("V1;...") == nil)
        #expect(parser.tryParse("") == nil)
    }

    @Test("Rejects unsupported processType")
    func rejectsUnsupportedProcessType() {
        // processType is "AVRechnung" instead of Kassenbeleg-V1 — must be ignored.
        let raw = "V0;CASH-1;AVRechnung;Beleg^0.00^9.00:Bar;42;1;a;b;c;d;e;f"
        #expect(parser.tryParse(raw) == nil)
    }

    @Test("Rejects malformed processData (fewer than 3 ^-separated parts)")
    func rejectsMalformedProcessData() {
        let raw = "V0;CASH-1;Kassenbeleg-V1;Beleg^9.00:Bar;42;1;a;b;c;d;e;f"
        #expect(parser.tryParse(raw) == nil)
    }

    @Test("Rejects when fewer than the required 12 fields are present")
    func rejectsShortFieldCount() {
        let raw = "V0;CASH-1;Kassenbeleg-V1;Beleg^0.00^9.00:Bar;42"
        #expect(parser.tryParse(raw) == nil)
    }
}
