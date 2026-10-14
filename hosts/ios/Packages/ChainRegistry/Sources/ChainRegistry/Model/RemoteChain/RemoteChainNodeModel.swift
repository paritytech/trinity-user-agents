import Foundation

public struct RemoteChainNodeModel: Equatable, Codable, Hashable {
    public let url: String
    public let name: String
    public let features: [String]?

    public init(url: String, name: String, features: [String]?) {
        self.url = url
        self.name = name
        self.features = features
    }
}
