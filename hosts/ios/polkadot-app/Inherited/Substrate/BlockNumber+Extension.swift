import Foundation
import SubstrateSdk

extension BlockNumber {
    func toHex() -> String {
        var blockNumber = self

        return Data(
            Data(bytes: &blockNumber, count: MemoryLayout<UInt32>.size).reversed()
        ).toHex(includePrefix: true)
    }
}
