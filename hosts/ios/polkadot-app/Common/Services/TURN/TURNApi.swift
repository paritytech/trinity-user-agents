import Foundation
import Operation_iOS

enum TURNApi {
    // swiftlint:disable:next type_name
    enum V1 {
        case issue(TURNIssueRequest)
    }
}

extension TURNApi.V1: URLConvertible {
    var url: URL {
        let components = URLComponents(string: urlString)
        guard let url = components?.url(relativeTo: AppConfig.Backend.baseUrl) else {
            assertionFailure()
            return AppConfig.Backend.baseUrl
        }
        return url
    }

    var httpMethod: String { HttpMethod.post.rawValue }

    var params: Encodable? {
        switch self {
        case let .issue(request): request
        }
    }

    private var urlString: String {
        switch self {
        case .issue: "api/v1/turn/issue"
        }
    }
}
