import Foundation
import Products
import SubstrateSdk
import TrUAPIHost

struct TrUAPIMediaPermissionSetting {
    let id: String
    let title: String
    let detail: String
    let request: PermissionAuthorizationRequest
    let status: PermissionAuthorizationStatus
}

/// Uses the process-owned runtime's canonical settings APIs. A read-only
/// execution derives the active Calling scope; settings never mutate through it.
struct TrUAPIMediaPermissionSettings: Sendable {
    let productId: ProductId

    func snapshot() async throws -> [TrUAPIMediaPermissionSetting] {
        let runtime = try sharedRuntime()
        let execution = try openExecution()
        defer { execution.close() }
        var requests: [PermissionAuthorizationRequest] = [.device(.microphone), .device(.camera)]
        requests.append(contentsOf: try await runtime.permissionAuthorizations(productId: productId).map(\.request))
        if let current = try? await execution.callingPermissionAuthorizationRequest() { requests.append(current) }
        var seen = Set<String>()
        var result: [TrUAPIMediaPermissionSetting] = []
        for request in requests {
            guard let display = display(request), seen.insert(display.id).inserted else { continue }
            let status = try await runtime.permissionAuthorizationStatus(productId: productId, request: request)
            var detail = display.detail
            if case let .device(device) = request, NativeMediaBackend.devicePermissionStatus(device) == .denied {
                detail += "\niOS access is blocked. Enable it in system Settings."
            }
            result.append(TrUAPIMediaPermissionSetting(id: display.id, title: display.title,
                detail: detail, request: request, status: status))
        }
        return result.sorted { $0.id < $1.id }
    }

    func set(_ setting: TrUAPIMediaPermissionSetting, allowed: Bool) async throws {
        guard display(setting.request) != nil else { throw NativeMediaError.BackendFailure }
        let runtime = try sharedRuntime()
        // The trusted settings switch is already the user's intent; do not ask
        // for it twice. Turning it off restores Ask and fences active capture.
        let status: PermissionAuthorizationStatus = allowed ? .authorized : .notDetermined
        try Task.checkCancellation()
        try await runtime.setPermissionAuthorizationStatus(productId: productId, request: setting.request, status: status)
    }

    private func sharedRuntime() throws -> TrUAPIHostRuntime {
        guard let provider: TrUAPIHostRuntimeProviding = RootDependencyLocator.getDependency() else {
            throw NativeMediaError.Closed
        }
        return try provider.sharedRuntime()
    }

    private func openExecution() throws -> TrUAPIProductExecution {
        try sharedRuntime().openProductExecution(
            bridge: SettingsBridge(productId: productId),
            configuration: ProductExecutionConfig(productId: productId, executionKind: .app)
        )
    }

    private func display(_ request: PermissionAuthorizationRequest) -> (id: String, title: String, detail: String)? {
        switch request {
        case .device(.microphone):
            return ("media:microphone", "Microphone", "\(productId) · During calls")
        case .device(.camera):
            return ("media:camera", "Camera", "\(productId) · During calls")
        case let .calling(network, account):
            return ("media:calling:\(network.toHex()):\(account.toHex())", "Calling",
                "\(productId) may place and receive calls with this profile.")
        default: return nil
        }
    }

    private final class SettingsBridge: HostBridge, @unchecked Sendable {
        let storage: HostStorageBackend
        let coreStorage: HostCoreStorageBackend
        init(productId: ProductId) {
            storage = ProductStorageBackend(storage: TrUAPILocalStorage.createProductLocalStorage(productId: productId))
            coreStorage = CoreStorageBackend(storage: TrUAPILocalStorage.createCoreLocalStorage())
        }
        func permissionAuthorizationsChanged(productId: String) {
            NotificationCenter.default.post(name: .productPermissionAuthorizationsChanged, object: productId)
        }
        func navigateTo(url: String) async throws { throw NativeMediaError.Closed }
        func devicePermission(
            product _: ProductExecutionConfig,
            request _: HostDevicePermissionRequest
        ) async throws -> TrUAPIPermissionDecision { .deny }
        func devicePermissionStatus(request: HostDevicePermissionRequest) async throws -> DevicePermissionStatus {
            NativeMediaBackend.devicePermissionStatus(request)
        }
        func remotePermission(
            product _: ProductExecutionConfig,
            request _: RemotePermission
        ) async throws -> TrUAPIPermissionDecision { .deny }
        func featureSupported(request: HostFeatureSupportedRequest) async throws -> Bool { false }
    }
}
