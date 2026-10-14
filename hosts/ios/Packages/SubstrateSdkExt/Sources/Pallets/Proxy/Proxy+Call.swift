import Foundation
import SubstrateSdk

public extension Proxy {
    struct ProxyCall: Codable {
        enum CodingKeys: String, CodingKey {
            case real
            case forceProxyType = "force_proxy_type"
            case call
        }

        public let real: MultiAddress
        public let forceProxyType: Proxy.ProxyType?
        public let call: JSON

        public init(real: MultiAddress, forceProxyType: Proxy.ProxyType?, call: JSON) {
            self.real = real
            self.forceProxyType = forceProxyType
            self.call = call
        }

        public static var callPath: CallCodingPath {
            CallCodingPath(moduleName: Proxy.name, callName: "proxy")
        }

        public func runtimeCall() -> RuntimeCall<Self> {
            RuntimeCall(
                moduleName: Self.callPath.moduleName,
                callName: Self.callPath.callName,
                args: self
            )
        }
    }

    struct AddProxyCall: Codable {
        enum CodingKeys: String, CodingKey {
            case proxy = "delegate"
            case proxyType = "proxy_type"
            case delay
        }

        public let proxy: MultiAddress
        public let proxyType: ProxyType
        @StringCodable public var delay: BlockNumber

        public init(proxy: MultiAddress, proxyType: ProxyType, delay: BlockNumber) {
            self.proxy = proxy
            self.proxyType = proxyType
            self.delay = delay
        }
    }

    typealias RemoveProxyCall = AddProxyCall
}
