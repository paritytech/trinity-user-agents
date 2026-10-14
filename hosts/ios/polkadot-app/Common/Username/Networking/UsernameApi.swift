import Foundation
import Operation_iOS
import UniqueDevice

enum UsernameApi {
    // swiftlint:disable:next type_name
    enum V1 {
        case attester
        case available(String)
        case register(RegisterUsernameParameters)
        case search(UsernameRequestModel)
        case notify(NotifyRequestParameters)
    }
}

extension UsernameApi.V1: URLConvertible {
    var url: URL {
        var components = URLComponents(string: urlString)
        components?.queryItems = queryItems
        guard let url = components?.url(relativeTo: AppConfig.Backend.baseUrl) else {
            assertionFailure()
            return AppConfig.Backend.baseUrl
        }
        return url
    }

    var httpMethod: String {
        switch self {
        case .attester,
             .search:
            HttpMethod.get.rawValue
        case .available,
             .register,
             .notify:
            HttpMethod.post.rawValue
        }
    }

    private var urlString: String {
        switch self {
        case .attester:
            "api/v1/attester"
        case .available:
            "api/v1/usernames/available"
        case .register:
            "api/v1/usernames"
        case .search:
            "api/v1/usernames/search"
        case .notify:
            "api/v1/notify"
        }
    }

    private var queryItems: [URLQueryItem]? {
        switch self {
        case let .search(usernameRequest):
            [URLQueryItem(name: "prefix", value: usernameRequest.prefix)]
        case .available:
            [URLQueryItem(name: "version", value: "v1")]
        default:
            nil
        }
    }

    var params: Encodable? {
        switch self {
        case .attester:
            nil
        case let .available(username):
            ["usernames": [username]]
        case let .register(params):
            params
        case .search:
            nil
        case let .notify(params):
            params
        }
    }
}
