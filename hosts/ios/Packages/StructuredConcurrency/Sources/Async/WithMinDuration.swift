import Foundation

/// Ensures `operation` takes at least `duration` to complete, sleeping the remainder if needed.
public func withMinDuration<T>(
    _ duration: Duration,
    operation: () async throws -> T
) async throws -> T {
    let clock = ContinuousClock()
    let start = clock.now

    do {
        let result = try await operation()
        let elapsed = clock.now - start
        if elapsed < duration {
            try await Task.sleep(for: duration - elapsed)
        }
        return result
    } catch {
        let elapsed = clock.now - start
        if elapsed < duration {
            try? await Task.sleep(for: duration - elapsed)
        }
        throw error
    }
}
