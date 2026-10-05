import Foundation
import TrUAPIHost

func truapiRecordChanges(_ name: Notification.Name) -> AsyncStream<Void> {
    AsyncStream { continuation in
        let observer = NotificationCenter.default.addObserver(forName: name, object: nil, queue: nil) { _ in
            continuation.yield(())
        }
        continuation.yield(())
        continuation.onTermination = { _ in NotificationCenter.default.removeObserver(observer) }
    }
}

extension Notification.Name {
    static let truapiPermissionsChanged = Notification.Name("truapiPermissionsChanged")
    static let truapiWorkerRecordsChanged = Notification.Name("truapiWorkerRecordsChanged")
}

func notifyRuntimeRecordsChanged() {
    let names: [Notification.Name] = [
        .truapiProductsChanged, .truapiPermissionsChanged, .truapiWorkerRecordsChanged, .truapiPairedHostsChanged
    ]
    for name in names {
        NotificationCenter.default.post(name: name, object: nil)
    }
}

func readActiveRuntimeRecords<Value>(
    empty: Value,
    read: (TrUAPIHostRuntime) async throws -> Value
) async throws -> Value {
    guard let provider: TrUAPIHostRuntimeProviding = RootDependencyLocator.getDependency() else {
        throw HostRejection.Rejected(reason: "Rust runtime provider unavailable")
    }
    guard let runtime = try await provider.activeRuntimeForRecords() else { return empty }
    do {
        return try await read(runtime)
    } catch {
        guard try await provider.activeRuntimeForRecords() != nil else { return empty }
        throw error
    }
}
