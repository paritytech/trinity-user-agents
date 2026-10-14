import Foundation
import SubstrateSdk
import BigInt

public extension OrmlPallet {
    enum Transfer {
        public static func codingPath(for moduleName: String) -> CallCodingPath {
            .init(moduleName: moduleName, callName: "transfer")
        }
    }

    enum TransferAll {
        public static func codingPath(for moduleName: String) -> CallCodingPath {
            .init(moduleName: moduleName, callName: "transfer_all")
        }
    }
}
