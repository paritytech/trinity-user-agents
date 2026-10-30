import Foundation

func waitUntil(_ condition: () async -> Bool) async throws {
    for _ in 0 ..< 50 {
        if await condition() { return }
        try await Task.sleep(for: .milliseconds(50))
    }
}
