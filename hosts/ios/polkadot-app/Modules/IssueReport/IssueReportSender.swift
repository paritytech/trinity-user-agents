import Foundation

struct IssueReport: Sendable {
    let description: String
    let logs: Data
    let screenshot: Data
}

protocol IssueReportSending: Sendable {
    /// Completes successfully only after the proxy confirms issue creation.
    func send(_ report: IssueReport) async throws
}

enum IssueReportSubmissionError: Error, Equatable {
    case unavailable
    case tooLarge
    case rejected
}

struct IssueProxyConfiguration: Sendable {
    let endpoint: URL
    let apiKey: String

    init(endpoint: String, apiKey: String) throws {
        guard
            endpoint.rangeOfCharacter(from: .whitespacesAndNewlines) == nil,
            let components = URLComponents(string: endpoint),
            components.scheme?.lowercased() == "https",
            let host = components.host, !host.isEmpty,
            components.user == nil, components.password == nil, components.fragment == nil,
            let url = components.url,
            !apiKey.isEmpty, apiKey.utf8.allSatisfy({ (33 ... 126).contains($0) })
        else {
            throw IssueReportSubmissionError.unavailable
        }
        self.endpoint = url
        self.apiKey = apiKey
    }
}

struct IssueProxyReportSender: IssueReportSending {
    private let configuration: @Sendable () async throws -> IssueProxyConfiguration
    private let client: any HTTPDataLoading

    init(
        configuration: @escaping @Sendable () async throws -> IssueProxyConfiguration,
        client: (any HTTPDataLoading)? = nil
    ) {
        self.configuration = configuration
        self.client = client ?? Self.session
    }

    func send(_ report: IssueReport) async throws {
        let configuration = try await configuration()
        try Task.checkCancellation()
        guard report.screenshot.count <= 10 * 1_024 * 1_024, report.logs.count <= 25 * 1_024 * 1_024 else {
            throw IssueReportSubmissionError.tooLarge
        }
        let boundary = "issue-\(UUID().uuidString)"
        let body = multipart(report, boundary: boundary)
        guard body.count <= 25 * 1_024 * 1_024 else {
            throw IssueReportSubmissionError.tooLarge
        }
        var request = URLRequest(
            url: configuration.endpoint,
            cachePolicy: .reloadIgnoringLocalCacheData,
            timeoutInterval: 300
        )
        request.httpMethod = "POST"
        request.setValue("Bearer \(configuration.apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue("multipart/form-data; boundary=\(boundary)", forHTTPHeaderField: "Content-Type")
        request.httpBody = body

        let (_, response) = try await client.data(for: request)
        try Task.checkCancellation()
        guard (response as? HTTPURLResponse)?.statusCode == 201 else {
            throw IssueReportSubmissionError.rejected
        }
    }
}

private extension IssueProxyReportSender {
    static let session: URLSession = {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.timeoutIntervalForRequest = 300
        configuration.timeoutIntervalForResource = 300
        configuration.waitsForConnectivity = false
        configuration.httpShouldSetCookies = false
        configuration.httpCookieStorage = nil
        configuration.urlCache = nil
        configuration.urlCredentialStorage = nil
        return URLSession(configuration: configuration, delegate: IssueReportRedirectDelegate(), delegateQueue: nil)
    }()

    func multipart(_ report: IssueReport, boundary: String) -> Data {
        let description = """
        \(report.description)

        ## App information

        - App ID: `\(Bundle.main.bundleIdentifier ?? "Unknown")`
        - App version: `\(Bundle.main.appVersion ?? "Unknown")`
        - Build: `\(Bundle.main.appBuild ?? "Unknown")`
        """
        var body = Data()
        func append(_ headers: String, data: Data) {
            body.append(Data("--\(boundary)\r\nContent-Disposition: form-data; \(headers)\r\n\r\n".utf8))
            body.append(data)
            body.append(Data("\r\n".utf8))
        }
        append("name=\"title\"", data: Data("iOS app issue".utf8))
        append("name=\"body\"", data: Data(description.utf8))
        append(
            "name=\"screenshot\"; filename=\"screenshot.png\"\r\nContent-Type: image/png",
            data: report.screenshot
        )
        append("name=\"logs\"; filename=\"logs.zip\"\r\nContent-Type: application/zip", data: report.logs)
        body.append(Data("--\(boundary)--\r\n".utf8))
        return body
    }
}

final class IssueReportRedirectDelegate: NSObject, URLSessionTaskDelegate {
    func urlSession(
        _: URLSession,
        task _: URLSessionTask,
        willPerformHTTPRedirection _: HTTPURLResponse,
        newRequest _: URLRequest,
        completionHandler: @escaping @Sendable (URLRequest?) -> Void
    ) {
        completionHandler(nil)
    }
}
