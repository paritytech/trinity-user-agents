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

/// Uses core-owned typed requests and its admin API, never the legacy permission
/// repository or a host interpretation of SCALE storage values. A short-lived
/// execution permits managing persisted scopes even while the product is closed.
struct TrUAPIMediaPermissionSettings: Sendable {
    static let didChange = Notification.Name("HostMediaCanonicalPermissionChanged")
    let productId: String

    func snapshot() async throws -> [TrUAPIMediaPermissionSetting] {
        let execution = try openExecution()
        defer { execution.close() }
        var requests: [PermissionAuthorizationRequest] = [.device(.microphone), .device(.camera)]
        requests.append(contentsOf: storedRequests())
        if let current = try? await execution.callingPermissionAuthorizationRequest() { requests.append(current) }
        var seen = Set<String>()
        var result: [TrUAPIMediaPermissionSetting] = []
        for request in requests {
            guard let display = display(request), seen.insert(display.id).inserted else { continue }
            let status = try await execution.permissionAuthorizationStatus(request: request)
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
        let execution = try openExecution()
        defer { execution.close() }
        let status: PermissionAuthorizationStatus
        if allowed {
            let presentation = await NativeMediaPresentation(productId: productId)
            do {
                let granted = try await withTaskCancellationHandler {
                    switch setting.request {
                    case let .calling(network, account):
                        return try await presentation.confirmCalling(network: network, account: account)
                    case .device(.microphone):
                        return try await presentation.confirm(title: "Allow microphone?",
                            detail: "\(productId) may use your microphone through host-managed Media.")
                    case .device(.camera):
                        return try await presentation.confirm(title: "Allow camera?",
                            detail: "\(productId) may use your camera through host-managed Media.")
                    default: throw NativeMediaError.BackendFailure
                    }
                } onCancel: {
                    Task { @MainActor in presentation.cancelPrompt() }
                }
                await presentation.close()
                status = granted ? .authorized : .denied
            } catch {
                await presentation.close()
                throw error
            }
        } else {
            // Turning off a remembered grant restores Ask, matching the existing
            // settings revoke action. The core atomically fences active capture.
            status = .notDetermined
        }
        try Task.checkCancellation()
        try await execution.setPermissionAuthorizationStatus(request: setting.request, status: status)
    }

    func revokeAll() async throws {
        let requests = storedRequests().filter { display($0) != nil }
        guard !requests.isEmpty else { return }
        let execution = try openExecution()
        defer { execution.close() }
        for request in requests {
            try await execution.setPermissionAuthorizationStatus(request: request, status: .notDetermined)
        }
    }

    private func storedRequests() -> [PermissionAuthorizationRequest] {
        TrUAPILocalStorage.createCoreLocalStorage().keys().compactMap { key in
            guard let encoded = try? Data(hexString: key),
                  let description = try? nativeDescribeCoreStorageKey(encoded: encoded),
                  description.productId == productId else { return nil }
            return description.permissionRequest
        }
    }

    private func openExecution() throws -> TrUAPIProductExecution {
        guard let provider: TrUAPIHostRuntimeProviding = RootDependencyLocator.getDependency() else {
            throw NativeMediaError.Closed
        }
        return try provider.sharedRuntime().openProductExecution(
            bridge: SettingsBridge(productId: productId),
            configuration: ProductExecutionConfig(productId: productId, executionKind: .app)
        )
    }

    private func display(_ request: PermissionAuthorizationRequest) -> (id: String, title: String, detail: String)? {
        switch request {
        case .device(.microphone):
            return ("media:microphone", "Microphone", "\(productId) · Host-managed Media microphone")
        case .device(.camera):
            return ("media:camera", "Camera", "\(productId) · Host-managed Media camera")
        case let .calling(network, account):
            return ("media:calling:\(network.toHex()):\(account.toHex())", "Calling",
                "\(productId)\nNetwork: \(network.toHex())\nAccount: \(account.toHex())")
        default: return nil
        }
    }

    private final class SettingsBridge: HostBridge, @unchecked Sendable {
        let storage: HostStorageBackend
        let coreStorage: HostCoreStorageBackend
        init(productId: String) {
            storage = ProductStorageBackend(storage: TrUAPILocalStorage.createProductLocalStorage(productId: productId))
            coreStorage = CoreStorageBackend(storage: TrUAPILocalStorage.createCoreLocalStorage())
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
