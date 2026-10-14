import Foundation
import os
@testable import polkadot_app

final class StubHTTPLoader: HTTPDataLoading, @unchecked Sendable {
    struct Stub {
        let statusCode: Int
        let body: Data
        let headers: [String: String]
        let delay: TimeInterval

        init(
            statusCode: Int = 200,
            body: Data = Data(),
            headers: [String: String] = ["Content-Type": "application/json"],
            delay: TimeInterval = 0
        ) {
            self.statusCode = statusCode
            self.body = body
            self.headers = headers
            self.delay = delay
        }
    }

    private struct State {
        var stubs: [String: Stub] = [:]
        var recorded: [URLRequest] = []
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    func setStub(_ stub: Stub, for url: URL) {
        state.withLock { $0.stubs[url.absoluteString] = stub }
    }

    var recordedRequests: [URLRequest] {
        state.withLock { $0.recorded }
    }

    func recordedRequests(for url: URL) -> [URLRequest] {
        state.withLock { state in
            state.recorded.filter { $0.url?.absoluteString == url.absoluteString }
        }
    }

    func requestCount(for url: URL) -> Int {
        recordedRequests(for: url).count
    }

    // MARK: - HTTPDataLoading

    func data(for request: URLRequest) async throws -> (Data, URLResponse) {
        let stub: Stub? = state.withLock { state in
            state.recorded.append(request)
            guard let url = request.url else { return nil }
            return state.stubs[url.absoluteString]
        }

        guard let stub, let url = request.url else {
            throw URLError(.unsupportedURL)
        }

        if stub.delay > 0 {
            try await Task.sleep(nanoseconds: UInt64(stub.delay * 1_000_000_000))
        }

        let response = HTTPURLResponse(
            url: url,
            statusCode: stub.statusCode,
            httpVersion: "HTTP/1.1",
            headerFields: stub.headers
        )!
        return (stub.body, response)
    }
}
