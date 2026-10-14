import Foundation
import SubstrateSdk

public enum LocalChainApiExternalType: String {
    case transactionHistory
    case hop
}

public struct LocalChainExternalApi: Equatable, Codable, Hashable {
    public let apiType: String
    public let serviceType: String
    public let url: URL
    public let parameters: JSON?

    public init(apiType: String, serviceType: String, url: URL, parameters: JSON?) {
        self.apiType = apiType
        self.serviceType = serviceType
        self.url = url
        self.parameters = parameters
    }

    public var identifier: String {
        Self.createId(from: apiType, serviceType: serviceType, url: url)
    }

    public static func createId(from apiType: String, serviceType: String, url: URL) -> String {
        apiType + "-" + serviceType + url.absoluteString
    }
}

public struct LocalChainExternalApiSet: Codable, Equatable, Hashable {
    public let apis: Set<LocalChainExternalApi>

    public func getApis(for type: LocalChainApiExternalType) -> Set<LocalChainExternalApi>? {
        let targetApis = apis.filter { LocalChainApiExternalType(rawValue: $0.apiType) == type }
        return !targetApis.isEmpty ? Set(targetApis) : nil
    }

    public func history() -> Set<LocalChainExternalApi>? {
        getApis(for: .transactionHistory)
    }

    public func hop() -> Set<LocalChainExternalApi>? {
        getApis(for: .hop)
    }

    public init(localApis: Set<LocalChainExternalApi>) {
        apis = localApis
    }

    public init(remoteApi: RemoteChainExternalApiSet) {
        apis = Set<LocalChainExternalApi>()
            .addingApis(from: remoteApi.transactionHistory, apiType: .transactionHistory)
            .addingNodeUrls(from: remoteApi.hop, apiType: .hop)
    }
}

public extension Set<LocalChainExternalApi> {
    func addingApis(from remoteApis: [RemoteChainExternalApi]?, apiType: LocalChainApiExternalType) -> Set<Element> {
        guard let remoteApis else {
            return self
        }

        let localApis = remoteApis.map {
            LocalChainExternalApi(
                apiType: apiType.rawValue,
                serviceType: $0.type,
                url: $0.url,
                parameters: $0.parameters
            )
        }

        return union(Set(localApis))
    }

    func addingNodeUrls(from urls: [URL]?, apiType: LocalChainApiExternalType) -> Set<Element> {
        guard let urls else {
            return self
        }

        let localApis = urls.map {
            LocalChainExternalApi(
                apiType: apiType.rawValue,
                serviceType: apiType.rawValue,
                url: $0,
                parameters: nil
            )
        }

        return union(Set(localApis))
    }
}
