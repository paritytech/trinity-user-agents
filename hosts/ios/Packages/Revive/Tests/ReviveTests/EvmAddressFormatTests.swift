import Foundation
import Testing
@testable import Revive

struct EvmAddressFormatTests {
    @Test("twenty bytes pass through unchanged")
    func validAddress() throws {
        let bytes = Data(repeating: 0xAB, count: 20)

        #expect(try EvmAddressFormat.validate(bytes) == bytes)
    }

    @Test("anything but twenty bytes is refused with its size", arguments: [0, 19, 21, 32])
    func invalidAddress(size: Int) {
        #expect(throws: EvmAddressError.invalidSize(size)) {
            try EvmAddressFormat.validate(Data(repeating: 0x01, count: size))
        }
    }

    @Test("the zero address is twenty zero bytes")
    func zero() {
        #expect(EvmAddressFormat.zero == Data(repeating: 0, count: 20))
        #expect(EvmAddressFormat.zero.count == EvmAddressFormat.size)
    }
}
