import Foundation
import Testing
@testable import polkadot_app

struct IssueProxyReportSenderTests {
    private let endpoint = "https://reports.example/v1/issues"
    private let report = IssueReport(
        description: "The screen froze 🐛",
        logs: Data([0x50, 0x4B, 0x03, 0x04]),
        screenshot: Data([0x89, 0x50, 0x4E, 0x47])
    )

    @Test func postsAuthenticatedProxyMultipart() async throws {
        let url = try #require(URL(string: endpoint))
        let client = StubHTTPLoader()
        client.setStub(.init(statusCode: 201), for: url)
        let configuration = try IssueProxyConfiguration(endpoint: endpoint, apiKey: "app-key")
        let sender = IssueProxyReportSender(configuration: { configuration }, client: client)

        try await sender.send(report)

        let request = try #require(client.recordedRequests.first)
        let contentType = try #require(request.value(forHTTPHeaderField: "Content-Type"))
        let boundary = try #require(contentType.components(separatedBy: "boundary=").last)
        let description = """
        The screen froze 🐛

        ## App information

        - App ID: `\(Bundle.main.bundleIdentifier ?? "Unknown")`
        - App version: `\(Bundle.main.appVersion ?? "Unknown")`
        - Build: `\(Bundle.main.appBuild ?? "Unknown")`
        """
        var body = Data((
            "--\(boundary)\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\niOS app issue\r\n" +
                "--\(boundary)\r\nContent-Disposition: form-data; name=\"body\"\r\n\r\n\(description)\r\n" +
                "--\(boundary)\r\nContent-Disposition: form-data; name=\"screenshot\"; " +
                "filename=\"screenshot.png\"\r\nContent-Type: image/png\r\n\r\n"
        ).utf8)
        body.append(report.screenshot)
        body.append(Data((
            "\r\n--\(boundary)\r\nContent-Disposition: form-data; name=\"logs\"; " +
                "filename=\"logs.zip\"\r\nContent-Type: application/zip\r\n\r\n"
        ).utf8))
        body.append(report.logs)
        body.append(Data("\r\n--\(boundary)--\r\n".utf8))

        var expected = URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData, timeoutInterval: 300)
        expected.httpMethod = "POST"
        expected.setValue("Bearer app-key", forHTTPHeaderField: "Authorization")
        expected.setValue("multipart/form-data; boundary=\(boundary)", forHTTPHeaderField: "Content-Type")
        expected.httpBody = body
        #expect(client.recordedRequests == [expected])
        #expect(request.httpBody == body)
    }

    @Test func missingOrUnsafeConfigurationMakesNoRequest() async {
        let client = StubHTTPLoader()
        let invalidValues = [
            ("", "app-key"),
            (endpoint, ""),
            ("http://reports.example/v1/issues", "app-key"),
            ("https://user:password@reports.example/v1/issues", "app-key"),
            ("https://reports.example/v1/issues#fragment", "app-key"),
            (endpoint, "app-key\r\nX-Other: value")
        ]
        for (endpoint, apiKey) in invalidValues {
            let sender = IssueProxyReportSender(
                configuration: { try IssueProxyConfiguration(endpoint: endpoint, apiKey: apiKey) },
                client: client
            )
            await #expect(throws: IssueReportSubmissionError.unavailable) {
                try await sender.send(report)
            }
        }
        #expect(client.recordedRequests.isEmpty)
    }

    @Test(arguments: [200, 202, 302, 401, 413, 500])
    func onlyCreatedIsSuccessful(status: Int) async throws {
        let url = try #require(URL(string: endpoint))
        let client = StubHTTPLoader()
        client.setStub(.init(statusCode: status), for: url)
        let configuration = try IssueProxyConfiguration(endpoint: endpoint, apiKey: "app-key")
        let sender = IssueProxyReportSender(configuration: { configuration }, client: client)

        await #expect(throws: IssueReportSubmissionError.rejected) {
            try await sender.send(report)
        }
        #expect(client.recordedRequests.count == 1)
    }
}
