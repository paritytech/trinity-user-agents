import Foundation
import SubstrateSdk

protocol ProxyCallFilterFactoryProtocol {
    func getProxyTypes(for call: CallCodingPath) -> Set<Proxy.ProxyType>
}

final class UnsupportedProxyFilterFactory: ProxyCallFilterFactoryProtocol {
    func getProxyTypes(for _: CallCodingPath) -> Set<Proxy.ProxyType> {
        fatalError("Implement proxy filter")
    }
}
