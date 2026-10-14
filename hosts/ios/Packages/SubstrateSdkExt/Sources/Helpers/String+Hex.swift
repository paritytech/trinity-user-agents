import Foundation
import SubstrateSdk

public extension String {
    func isHex() -> Bool {
        hasPrefix("0x") && lengthOfBytes(using: .ascii) % 2 == 0
    }

    func withHexPrefix() -> String {
        guard !hasPrefix("0x") else {
            return self
        }
        return "0x" + self
    }

    func withoutHexPrefix() -> String {
        if hasPrefix("0x") {
            let indexStart = index(startIndex, offsetBy: 2)
            return String(self[indexStart...])
        } else {
            return self
        }
    }

    func fromHex() throws -> Data {
        try Data(hexString: self)
    }
}
