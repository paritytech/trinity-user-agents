import Foundation
import Foundation_iOS

public actor AsyncValue<Value> {
    private var storage: UncertainStorage<Value> = .undefined
    private var waiters: [UUID: CheckedContinuation<Value, Error>] = [:]

    public init() {}

    public func get() async throws -> Value {
        switch storage {
        case let .defined(value):
            return value
        case .undefined:
            let identifier = UUID()

            return try await withTaskCancellationHandler(operation: {
                try await withCheckedThrowingContinuation { continuation in
                    waiters[identifier] = continuation
                }
            }, onCancel: {
                Task {
                    await self.cancelTask(with: identifier)
                }
            })
        }
    }

    public func set(_ newValue: Value) {
        storage = .defined(newValue)

        let toResume = waiters
        waiters.removeAll()
        for continuation in toResume.values {
            continuation.resume(returning: newValue)
        }
    }

    public func reset() {
        storage = .undefined
    }

    public var isDefined: Bool {
        switch storage {
        case .defined:
            true
        case .undefined:
            false
        }
    }

    private func cancelTask(with identifier: UUID) {
        guard let continuation = waiters.removeValue(forKey: identifier) else {
            return
        }

        continuation.resume(throwing: CancellationError())
    }
}
