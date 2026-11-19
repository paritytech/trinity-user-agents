import Foundation
import Operation_iOS
import UniqueDevice

protocol TURNCredentialsRequestMaking: Sendable {
    func issueCredentials(
        request: TURNIssueRequest,
        modifier: HttpRequestModifier?
    ) async throws -> TURNCredentials
}

final class TURNCredentialsRequestFactory: TURNCredentialsRequestMaking {
    private let urlSession: any HTTPDataLoading

    init(urlSession: any HTTPDataLoading = URLSession.shared) {
        self.urlSession = urlSession
    }

    func issueCredentials(
        request: TURNIssueRequest,
        modifier: HttpRequestModifier? = nil
    ) async throws -> TURNCredentials {
        let endpoint = TURNApi.V1.issue(request)
        let body = try endpoint.params.map { try JSONEncoder().encode($0) }

        var urlRequest = URLRequest(url: endpoint.url)
        urlRequest.httpMethod = endpoint.httpMethod
        urlRequest.setValue(
            HttpContentType.json.rawValue,
            forHTTPHeaderField: HttpHeaderKey.contentType.rawValue
        )
        urlRequest.httpBody = body

        try modifier?.visit(request: &urlRequest)

        let (data, urlResponse) = try await urlSession.data(for: urlRequest)

        // Rejections surface as `BackendApiError`, so a 401 makes `withAuthorizedToken` replace the token.
        return try UsernameJsonResultFactory<TURNCredentials>()
            .createResult(data: data, response: urlResponse, error: nil)
            .get()
    }
}
