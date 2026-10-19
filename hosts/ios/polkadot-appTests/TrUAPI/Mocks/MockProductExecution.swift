import Foundation
import TrUAPIHost

/// Test double for one product execution. Records ws-bridge lifecycle and
/// chain notify-backs; unused notify surfaces are inert.
final class MockProductExecution: TrUAPIProductExecutionProtocol, @unchecked Sendable {
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

    /// Card lists pushed to the product, in order, so a test can assert the
    /// worker was told what it holds rather than only that it started.
    private(set) var pocketCardNotifications: [[PocketCard]] = []

    private(set) var publishedChatActions: [HostChatActionSubscribeItem] = []
    private(set) var publishedRendererActions: [HostRendererActionSubscribeItem] = []
    /// Requests passed to `render`, in order, including ones that threw, so
    /// `renderRequests.count` is the call count the retry tests assert on.
    private(set) var renderRequests: [ProductRendererRenderRequest] = []
    /// The continuation behind each stream `render` returned, in order, so a
    /// test can push a face into a stream that is already being drained.
    private(set) var renderContinuations: [AsyncThrowingStream<RendererNode, Error>.Continuation] = []
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
    private let pocketCardSignal: AsyncStream<[PocketCard]>.Continuation
    private var pocketCardSignals: AsyncStream<[PocketCard]>.AsyncIterator
    private let themeSignal: AsyncStream<Void>.Continuation
    private var themeSignals: AsyncStream<Void>.AsyncIterator

    init() {
        let (renders, renderContinuation) = AsyncStream.makeStream(of: Void.self)
        let (closes, closeContinuation) = AsyncStream.makeStream(of: Void.self)
        let (cardLists, cardListContinuation) = AsyncStream.makeStream(of: [PocketCard].self)
        let (themes, themeContinuation) = AsyncStream.makeStream(of: Void.self)
        renderSignal = renderContinuation
        renderSignals = renders.makeAsyncIterator()
        closeSignal = closeContinuation
        closeSignals = closes.makeAsyncIterator()
        pocketCardSignal = cardListContinuation
        pocketCardSignals = cardLists.makeAsyncIterator()
        themeSignal = themeContinuation
        themeSignals = themes.makeAsyncIterator()
    }

    /// Returns once a `render` call has happened that no earlier call to this method consumed.
    func awaitRenderRequest() async {
        _ = await renderSignals.next()
    }

    /// Returns once a `close` call has happened that no earlier call to this method consumed.
    func awaitClose() async {
        _ = await closeSignals.next()
    }

    /// Returns the next card list pushed to the product that no earlier call to this method consumed.
    func awaitPocketCardNotification() async -> [PocketCard]? {
        await pocketCardSignals.next()
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

    func publishChatAction(_ item: HostChatActionSubscribeItem) throws {
        publishedChatActions.append(item)
    }

    func render(_ request: ProductRendererRenderRequest) throws -> AsyncThrowingStream<RendererNode, Error> {
        renderRequests.append(request)
        if !renderErrors.isEmpty {
            renderSignal.yield()
            throw renderErrors.removeFirst()
        }

        let nodes = renderNodes
        let staysOpen = keepsRenderStreamOpen
        let stream = AsyncThrowingStream<RendererNode, Error> { continuation in
            nodes.forEach { continuation.yield($0) }
            if !staysOpen { continuation.finish() }
            self.renderContinuations.append(continuation)
        }
        renderSignal.yield()
        return stream
    }

    func publishRendererAction(_ item: HostRendererActionSubscribeItem) throws {
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

    func notifyChatRoomsChanged(rooms _: [ChatRoom]) {}

    func sessionChatIdentityKey() throws -> Data? {
        nil
    }

    func notifyPocketCardsChanged(cards: [PocketCard]) {
        pocketCardNotifications.append(cards)
        pocketCardSignal.yield(cards)
    }
}
