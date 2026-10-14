import Foundation
import SubstrateSdk

public extension PeopleLitePallet {
    struct DispatchAsSignerCall<T: Codable>: Codable {
        let call: RuntimeCall<T>

        public init(call: RuntimeCall<T>) {
            self.call = call
        }

        public func runtimeCall() -> RuntimeCall<Self> {
            RuntimeCall(
                moduleName: PeopleLitePallet.name,
                callName: "dispatch_as_signer",
                args: self
            )
        }
    }
}
