import Foundation
import SubstrateSdk

public extension TransactionStoragePallet {
    struct StoreCall: Codable {
        @BytesCodable public var data: Data

        public init(data: Data) {
            _data = BytesCodable(wrappedValue: data)
        }

        public func runtimeCall() -> RuntimeCall<Self> {
            RuntimeCall(
                moduleName: TransactionStoragePallet.name,
                callName: "store",
                args: self
            )
        }
    }
}
