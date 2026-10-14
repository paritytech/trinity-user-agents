import Foundation

public struct ChainNodeModel: Equatable, Codable, Hashable {
    public enum Feature: String, Codable, Hashable {
        case alchemyApi
    }

    public let url: String
    public let name: String
    public let order: Int16
    public let features: Set<Feature>?

    public init(url: String, name: String, order: Int16, features: Set<Feature>?) {
        self.url = url
        self.name = name
        self.order = order
        self.features = features
    }

    public init(remoteModel: RemoteChainNodeModel, order: Int16) {
        url = remoteModel.url
        name = remoteModel.name
        self.order = order
        features = remoteModel.features.flatMap { Set($0.compactMap { Feature(rawValue: $0) }) }
    }
}
