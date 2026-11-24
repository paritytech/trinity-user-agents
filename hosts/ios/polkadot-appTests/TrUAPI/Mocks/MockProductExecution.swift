import Foundation
import TrUAPIHost

/// Test double for one product execution. Records ws-bridge lifecycle and
/// chain notify-backs; unused notify surfaces are inert.
final class MockProductExecution: NativeWorkerExecution, TrUAPIProductExecutionProtocol, @unchecked Sendable {
    private(set) var startWsBridgeCallCount = 0
    private(set) var stopWsBridgeCallCount = 0
    private(set) var closeCallCount = 0
    private(set) var chainResponses: [(UInt32, String)] = []
    private(set) var chainClosed: [UInt32] = []
    private(set) var themeChanges: [HostThemeSubscribeItem] = []

    /// Status returned by `permissionAuthorizationStatus`; defaults to
    /// `.notDetermined` so existing tests are unaffected.
    var permissionStatus: PermissionAuthorizationStatus = .notDetermined
    private(set) var permissionRequests: [PermissionAuthorizationRequest] = []
    /// Answer returned by `authorizeDevicePermission`.
    var devicePermissionGranted = false
    private(set) var devicePermissionRequests: [HostDevicePermissionRequest] = []

    private(set) var publishedChatActions: [HostChatActionSubscribeItem] = []
    private(set) var publishedRendererActions: [HostRendererActionSubscribeItem] = []
    /// Requests passed to `render`, in order, including ones that threw, so
    /// `renderRequests.count` is the call count the retry tests assert on.
    private(set) var renderRequests: [ProductRendererRenderRequest] = []
    private(set) var renderObservers: [NativeRendererObserver] = []
    /// Errors thrown by successive `render` calls, consumed in order; once
    /// empty the call succeeds. Lets tests drive the startup retry loop.
    var renderErrors: [Error] = []
    /// Nodes the render stream yields before finishing.
    var renderNodes: [RendererNode] = []
    /// Keeps the render stream open after those nodes, the way a live worker
    /// does. A stream that finishes is reopened after a wall-clock backoff, so
    /// a test counting `render` calls has to hold it open to be deterministic.
    var keepsRenderStreamOpen = false

    private let renderSignal: AsyncStream<Void>.Continuation
    private var renderSignals: AsyncStream<Void>.AsyncIterator
    private let closeSignal: AsyncStream<Void>.Continuation
    private var closeSignals: AsyncStream<Void>.AsyncIterator
    private let themeSignal: AsyncStream<Void>.Continuation
    private var themeSignals: AsyncStream<Void>.AsyncIterator

    init() {
        let (renders, renderContinuation) = AsyncStream.makeStream(of: Void.self)
        let (closes, closeContinuation) = AsyncStream.makeStream(of: Void.self)
        let (themes, themeContinuation) = AsyncStream.makeStream(of: Void.self)
        renderSignal = renderContinuation
        renderSignals = renders.makeAsyncIterator()
        closeSignal = closeContinuation
        closeSignals = closes.makeAsyncIterator()
        themeSignal = themeContinuation
        themeSignals = themes.makeAsyncIterator()
        super.init(noHandle: .init())
    }

    required init(unsafeFromHandle _: UInt64) { fatalError("Test execution") }

    /// Returns once a `render` call has happened that no earlier call to this method consumed.
    func awaitRenderRequest() async {
        _ = await renderSignals.next()
    }

    /// Returns once a `close` call has happened that no earlier call to this method consumed.
    func awaitClose() async {
        _ = await closeSignals.next()
    }

    /// Returns once a theme notification has happened that no earlier call to this method consumed.
    func awaitThemeChange() async {
        _ = await themeSignals.next()
    }

    func startWsBridge(bindPort _: UInt16) throws -> WsBridgeEndpoint {
        startWsBridgeCallCount += 1
        return WsBridgeEndpoint(port: 0, token: "test")
    }

    func stopWsBridge() {
        stopWsBridgeCallCount += 1
    }

    func close() {
        closeCallCount += 1
        closeSignal.yield()
    }

    override func publishChatAction(action: HostChatActionSubscribeItem) throws {
        publishedChatActions.append(action)
    }

    override func render(
        request: ProductRendererRenderRequest,
        observer: NativeRendererObserver
    ) throws -> NativeSubscription {
        renderRequests.append(request)
        if !renderErrors.isEmpty {
            renderSignal.yield()
            throw renderErrors.removeFirst()
        }

        renderObservers.append(observer)
        renderNodes.forEach { observer.onUpdate(node: $0) }
        if !keepsRenderStreamOpen { observer.onComplete() }
        renderSignal.yield()
        return RenderSubscription(noHandle: .init())
    }

    override func publishRendererAction(item: HostRendererActionSubscribeItem) throws {
        publishedRendererActions.append(item)
    }

    func permissionAuthorizationStatus(
        request: PermissionAuthorizationRequest
    ) async throws -> PermissionAuthorizationStatus {
        permissionRequests.append(request)
        return permissionStatus
    }

    func authorizeDevicePermission(_ request: HostDevicePermissionRequest) async throws -> Bool {
        devicePermissionRequests.append(request)
        return devicePermissionGranted
    }

    func setPermissionAuthorizationStatus(
        request _: PermissionAuthorizationRequest,
        status _: PermissionAuthorizationStatus
    ) throws {}

    func notifyThemeChanged(theme: HostThemeSubscribeItem) {
        themeChanges.append(theme)
        themeSignal.yield()
    }

    func notifyLocaleChanged(locale _: HostLocaleSubscribeItem) {}
    func notifyStorageChanged(key _: String, value _: Data?) {}
    func notifyPreimageChanged(key _: Data, value _: Data?) {}

    func notifyChainResponse(connectionId: UInt32, json: String) {
        chainResponses.append((connectionId, json))
    }

    func notifyChainClosed(connectionId: UInt32) {
        chainClosed.append(connectionId)
    }

    override func notifyChatRoomsChanged(rooms _: [ChatRoom]) {}

    func sessionChatIdentityKey() throws -> Data? {
        nil
    }
}

private final class RenderSubscription: NativeSubscription, @unchecked Sendable {
    override func cancel() {}
}
