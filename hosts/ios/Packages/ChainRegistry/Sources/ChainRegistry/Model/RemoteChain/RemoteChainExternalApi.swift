import Foundation
import SubstrateSdk

public struct RemoteChainExternalApi: Equatable, Codable {
    public let type: String
    public let url: URL
    public let parameters: JSON?

    public init(type: String, url: URL, parameters: JSON?) {
        self.type = type
        self.url = url
        self.parameters = parameters
    }
}

public struct RemoteChainExternalApiSet: Equatable, Codable {
    public enum CodingKeys: String, CodingKey {
        case transactionHistory
        case hop
    }

    public let transactionHistory: [RemoteChainExternalApi]?
    public let hop: [URL]?

    public init(transactionHistory: [RemoteChainExternalApi]?, hop: [URL]?) {
        self.transactionHistory = transactionHistory
        self.hop = hop
    }
}
