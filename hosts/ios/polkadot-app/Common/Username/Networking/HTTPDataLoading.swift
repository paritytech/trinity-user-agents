import Foundation

protocol HTTPDataLoading: Sendable {
    func data(for request: URLRequest) async throws -> (Data, URLResponse)
}

extension URLSession: HTTPDataLoading {}
