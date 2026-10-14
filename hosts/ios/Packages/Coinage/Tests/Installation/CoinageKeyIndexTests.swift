import Foundation
import Testing
@testable import Coinage

struct CoinageKeyIndexTests {
    @Test("the string form is the installation hex and the item, and reads back to the same index")
    func roundTrip() throws {
        let index = CoinageKeyIndex(installation: .test, item: 42)

        #expect(index.toString() == "\(CoinageInstallationId.test.hex)/42")
        #expect(try CoinageKeyIndex.fromString(index.toString()) == index)
    }

    @Test("the largest item survives the round trip")
    func maxItem() throws {
        let index = CoinageKeyIndex(installation: .other, item: .max)

        #expect(try CoinageKeyIndex.fromString(index.toString()) == index)
    }

    @Test(
        "a string that is not an index is refused",
        arguments: ["", "42", "/42", "abc/42", "\(CoinageInstallationId.test.hex)/x"]
    )
    func malformed(string: String) {
        #expect(throws: (any Error).self) { try CoinageKeyIndex.fromString(string) }
    }
}
