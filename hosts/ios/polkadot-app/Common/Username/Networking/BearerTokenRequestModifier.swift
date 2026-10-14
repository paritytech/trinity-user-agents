import Foundation
import UniqueDevice

final class BearerTokenRequestModifier: HttpRequestModifier {
    private let token: String

    init(token: String) {
        self.token = token
    }

    func visit(request: inout URLRequest) throws {
        guard !token.isEmpty else { return }
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
    }
}
