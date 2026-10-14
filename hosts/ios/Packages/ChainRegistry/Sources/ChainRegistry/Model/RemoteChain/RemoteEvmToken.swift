import Foundation
import SubstrateSdk

public struct RemoteEvmToken: Codable {
    public let symbol: String
    public let precision: Int
    public let name: String
    public let priceId: String?
    public let icon: String?
    public let instances: [Instance]

    public init(
        symbol: String,
        precision: Int,
        name: String,
        priceId: String?,
        icon: String?,
        instances: [Instance]
    ) {
        self.symbol = symbol
        self.precision = precision
        self.name = name
        self.priceId = priceId
        self.icon = icon
        self.instances = instances
    }

    public struct Instance: Codable {
        public let chainId: String
        public let contractAddress: String
        public let buyProviders: JSON?

        public init(chainId: String, contractAddress: String, buyProviders: JSON?) {
            self.chainId = chainId
            self.contractAddress = contractAddress
            self.buyProviders = buyProviders
        }
    }
}
