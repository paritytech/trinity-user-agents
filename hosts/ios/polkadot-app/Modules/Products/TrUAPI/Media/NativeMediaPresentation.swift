import UIKit
import WebKit
import WebRTC
import TrUAPIHost
import ReplayKit
import CallKit
import Metal
import AVFoundation
import SubstrateSdk

@MainActor
final class NativeMediaPresentation: NSObject, CXProviderDelegate {
    private final class ControlWindow: UIWindow {
        override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
            let hit = super.hitTest(point, with: event)
            return hit === rootViewController?.view ? nil : hit
        }
    }

    let productId: String
    var onEnd: (() -> Void)?
    var onScreenStop: (() -> Void)?
    var onAudioActivation: ((Bool) -> Void)?
    var onMute: ((Bool) -> Void)?
    private var window: UIWindow?
    private var bar: UIStackView?
    private var provider: CXProvider?
    private var callId: UUID?
    private var alert: UIAlertController?
    private var decision: CheckedContinuation<Bool, Error>?
    private var picker: UIViewController?

    nonisolated init(productId: String) { self.productId = productId; super.init() }

    static var available: Bool {
        #if targetEnvironment(simulator) || targetEnvironment(macCatalyst)
        return false
        #else
        guard MTLCreateSystemDefaultDevice() != nil,
              let group = MediaBroadcastMailbox.group,
              FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group) != nil,
              let plugins = Bundle.main.builtInPlugInsURL,
              let bundles = try? FileManager.default.contentsOfDirectory(at: plugins, includingPropertiesForKeys: nil),
              let identifier = Bundle.main.object(forInfoDictionaryKey: "MediaBroadcastExtension") as? String else { return false }
        return bundles.contains { Bundle(url: $0)?.bundleIdentifier == identifier }
        #endif
    }

    private func ensureWindow() throws -> UIWindow {
        if let window { return window }
        guard let scene = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene })
            .first(where: { $0.activationState == .foregroundActive || $0.activationState == .foregroundInactive }) else {
            throw NativeMediaFailure.domain(.deviceUnavailable)
        }
        let window = ControlWindow(windowScene: scene)
        window.windowLevel = .alert + 1
        window.rootViewController = UIViewController()
        window.rootViewController?.view.backgroundColor = .clear
        window.isHidden = false
        self.window = window
        return window
    }

    func show(pending: Bool, state: NativeMediaLocalState?) throws {
        let window = try ensureWindow()
        bar?.removeFromSuperview()
        let label = UILabel()
        label.numberOfLines = 0
        label.font = .preferredFont(forTextStyle: .caption1)
        label.textColor = .white
        label.text = "\(productId) — \(pending ? "Preparing media" : "Media active")"
        if let state {
            func name(_ value: NativeMediaTrackState) -> String {
                switch value { case .off: "off"; case .starting: "starting"; case .live: "on"; case .interrupted: "interrupted" }
            }
            label.text? += "\nMic \(name(state.microphone)) · Camera \(name(state.camera)) · Screen \(name(state.screen))"
        }
        label.accessibilityLabel = label.text
        let end = UIButton(type: .system)
        end.setTitle("End", for: .normal)
        end.addAction(UIAction { [weak self] _ in self?.onEnd?() }, for: .touchUpInside)
        let revoke = UIButton(type: .system)
        revoke.setTitle("Revoke calling", for: .normal)
        revoke.addAction(UIAction { [weak self] _ in
            guard let self else { return }
            NotificationCenter.default.post(name: NativeMediaBackend.permissionRevokedNotification, object: nil,
                userInfo: ["productId": self.productId, "permission": NativeMediaRevokedPermission.calling])
        }, for: .touchUpInside)
        let controls = UIStackView(arrangedSubviews: [end, revoke])
        controls.spacing = 16
        let stack = UIStackView(arrangedSubviews: [label, controls])
        stack.axis = .vertical
        if state?.screen != .off, state != nil {
            let stop = UIButton(type: .system)
            stop.setTitle("Stop screen", for: .normal)
            stop.addAction(UIAction { [weak self] _ in self?.onScreenStop?() }, for: .touchUpInside)
            controls.addArrangedSubview(stop)
        }
        stack.spacing = 12
        stack.backgroundColor = UIColor.black.withAlphaComponent(0.95)
        stack.isLayoutMarginsRelativeArrangement = true
        stack.layoutMargins = UIEdgeInsets(top: 8, left: 12, bottom: 8, right: 12)
        stack.translatesAutoresizingMaskIntoConstraints = false
        guard let root = window.rootViewController?.view else { throw NativeMediaFailure.domain(.deviceUnavailable) }
        root.addSubview(stack)
        NSLayoutConstraint.activate([stack.topAnchor.constraint(equalTo: root.safeAreaLayoutGuide.topAnchor),
            stack.leadingAnchor.constraint(equalTo: root.leadingAnchor), stack.trailingAnchor.constraint(equalTo: root.trailingAnchor)])
        bar = stack
    }

    func beginSystemCall() async throws {
        guard callId == nil else { return }
        let configuration = CXProviderConfiguration()
        configuration.maximumCallGroups = 1
        configuration.maximumCallsPerCallGroup = 1
        configuration.supportedHandleTypes = [.generic]
        configuration.supportsVideo = true
        let provider = CXProvider(configuration: configuration)
        provider.setDelegate(self, queue: .main)
        self.provider = provider
        let id = UUID()
        callId = id
        do {
            try await CXCallController().request(CXTransaction(action: CXStartCallAction(call: id,
                handle: CXHandle(type: .generic, value: productId))))
            guard callId == id else { throw NativeMediaFailure.cancelled }
            let update = CXCallUpdate()
            update.localizedCallerName = productId
            update.supportsHolding = false
            update.supportsGrouping = false
            update.supportsUngrouping = false
            update.supportsDTMF = false
            provider.reportCall(with: id, updated: update)
            provider.reportOutgoingCall(with: id, connectedAt: Date())
        } catch {
            provider.invalidate(); self.provider = nil; callId = nil
            throw NativeMediaFailure.domain(.deviceUnavailable)
        }
    }

    func confirmCalling(network: Data, account: Data) async throws -> Bool {
        try await confirm(title: "Allow calling?", detail:
            "Product: \(productId)\nNetwork genesis: \(network.toHex(includePrefix: true))\nAccount (sr25519): \(account.toHex(includePrefix: true))\n\nAllow this exact product, network and account to place and receive calls? Camera, microphone and screen sharing are separate choices.")
    }

    func confirm(title: String, detail: String) async throws -> Bool {
        guard decision == nil, UIApplication.shared.applicationState == .active else {
            throw NativeMediaFailure.domain(.deviceUnavailable)
        }
        let window = try ensureWindow()
        return try await withCheckedThrowingContinuation { continuation in
            decision = continuation
            let alert = UIAlertController(title: title, message: detail, preferredStyle: .alert)
            alert.addAction(UIAlertAction(title: "Deny", style: .destructive) { [weak self] _ in self?.resolve(.success(false)) })
            alert.addAction(UIAlertAction(title: "Allow", style: .default) { [weak self] _ in self?.resolve(.success(true)) })
            alert.addAction(UIAlertAction(title: "Not now", style: .cancel) { [weak self] _ in
                self?.resolve(.failure(NativeMediaFailure.cancelled))
            })
            self.alert = alert
            window.rootViewController?.present(alert, animated: true)
        }
    }

    private func resolve(_ result: Result<Bool, Error>) {
        let continuation = decision
        decision = nil; alert = nil
        continuation?.resume(with: result)
        if bar == nil { window?.isHidden = true; window = nil }
    }

    func cancelPrompt() {
        alert?.dismiss(animated: false)
        resolve(.failure(NativeMediaFailure.cancelled))
        picker?.dismiss(animated: false); picker = nil
    }

    func showScreenPicker() throws {
        guard UIApplication.shared.applicationState == .active,
              let identifier = Bundle.main.object(forInfoDictionaryKey: "MediaBroadcastExtension") as? String else {
            throw NativeMediaFailure.domain(.deviceUnavailable)
        }
        let root = try ensureWindow().rootViewController
        let sheet = UIViewController()
        sheet.view.backgroundColor = .systemBackground
        sheet.modalPresentationStyle = .pageSheet
        sheet.isModalInPresentation = true
        let title = UILabel()
        title.text = "Share your screen with \(productId)\nTap the system broadcast control to select and start sharing."
        title.numberOfLines = 0
        // Screen sharing must not enable ReplayKit's separate mic/camera paths.
        RPScreenRecorder.shared().isMicrophoneEnabled = false
        RPScreenRecorder.shared().isCameraEnabled = false
        let picker = RPSystemBroadcastPickerView(frame: CGRect(x: 0, y: 0, width: 64, height: 64))
        picker.preferredExtension = identifier
        picker.showsMicrophoneButton = false
        NSLayoutConstraint.activate([picker.widthAnchor.constraint(equalToConstant: 64),
            picker.heightAnchor.constraint(equalToConstant: 64)])
        let cancel = UIButton(type: .system)
        cancel.setTitle("Cancel screen sharing", for: .normal)
        cancel.addAction(UIAction { [weak self] _ in self?.onScreenStop?() }, for: .touchUpInside)
        let stack = UIStackView(arrangedSubviews: [title, picker, cancel])
        stack.axis = .vertical; stack.alignment = .center; stack.spacing = 24
        stack.translatesAutoresizingMaskIntoConstraints = false
        sheet.view.addSubview(stack)
        NSLayoutConstraint.activate([stack.centerYAnchor.constraint(equalTo: sheet.view.centerYAnchor),
            stack.leadingAnchor.constraint(equalTo: sheet.view.leadingAnchor, constant: 24),
            stack.trailingAnchor.constraint(equalTo: sheet.view.trailingAnchor, constant: -24)])
        self.picker = sheet
        root?.present(sheet, animated: true)
    }

    func dismissPicker() { picker?.dismiss(animated: true); picker = nil }

    func close() {
        cancelPrompt()
        if let callId { provider?.reportCall(with: callId, endedAt: Date(), reason: .remoteEnded) }
        callId = nil; provider?.invalidate(); provider = nil
        bar?.removeFromSuperview(); bar = nil
        window?.isHidden = true; window = nil
    }

    nonisolated func providerDidReset(_ provider: CXProvider) {
        Task { @MainActor in
            guard self.provider === provider else { return }
            self.onEnd?()
        }
    }
    nonisolated func provider(_ provider: CXProvider, perform action: CXStartCallAction) {
        Task { @MainActor in
            guard self.provider === provider, self.callId == action.callUUID else { action.fail(); return }
            action.fulfill()
        }
    }
    nonisolated func provider(_ provider: CXProvider, perform action: CXEndCallAction) {
        Task { @MainActor in
            if self.provider === provider, self.callId == action.callUUID { self.onEnd?() }
            action.fulfill()
        }
    }
    nonisolated func provider(_ provider: CXProvider, perform action: CXSetMutedCallAction) {
        Task { @MainActor in
            guard self.provider === provider, self.callId == action.callUUID else { action.fail(); return }
            self.onMute?(action.isMuted)
            action.fulfill()
        }
    }
    nonisolated func provider(_ provider: CXProvider, didActivate audioSession: AVAudioSession) {
        Task { @MainActor in
            guard self.provider === provider else { return }
            self.onAudioActivation?(true)
        }
    }
    nonisolated func provider(_ provider: CXProvider, didDeactivate audioSession: AVAudioSession) {
        Task { @MainActor in
            guard self.provider === provider else { return }
            self.onAudioActivation?(false)
        }
    }
}

@MainActor
final class NativeMediaCompositor: NSObject {
    private struct Picture {
        let surface: NativeMediaSurface
        let view: UIView
        var renderer: RTCMTLVideoView?
        var track: RTCVideoTrack?
    }
    private weak var product: WKWebView?
    private let below = UIView(), above = UIView()
    private var pictures: [Picture] = []
    private var displayLink: CADisplayLink?
    private var lastFrame = CGRect.null
    private weak var lastWindow: UIWindow?
    private var lastZoom: CGFloat = 0
    private var lastDeviceScale: CGFloat = 0
    private(set) var viewport: NativeMediaViewport?
    private var revision: UInt64 = 0
    private var layoutRevision: UInt64 = 0
    var onViewport: ((NativeMediaViewport?) -> Void)?

    func attach(_ product: WKWebView) {
        detach()
        self.product = product
        product.isOpaque = false
        product.backgroundColor = .clear
        below.isUserInteractionEnabled = false; above.isUserInteractionEnabled = false
        below.clipsToBounds = true; above.clipsToBounds = true
        let link = CADisplayLink(target: self, selector: #selector(refreshViewport))
        link.add(to: .main, forMode: .common)
        displayLink = link
        refreshViewport()
    }

    func detach() {
        displayLink?.invalidate(); displayLink = nil
        clear(); below.removeFromSuperview(); above.removeFromSuperview()
        product = nil; lastWindow = nil; lastFrame = .null; lastZoom = 0
        revision += 1; viewport = nil; onViewport?(nil)
    }

    @objc private func refreshViewport() {
        guard let product, let parent = product.superview, let window = product.window,
              visibleInHierarchy(product), product.transform.isIdentity,
              product.bounds.width > 0, product.bounds.height > 0,
              product.scrollView.zoomScale.isFinite, product.scrollView.zoomScale > 0 else {
            if viewport != nil { clear(); viewport = nil; revision += 1; onViewport?(nil) }
            return
        }
        let frame = product.convert(product.bounds, to: parent)
        let zoom = product.scrollView.zoomScale
        let deviceScale = window.screen.scale * zoom
        let width = frame.width / zoom, height = frame.height / zoom, scale = deviceScale * 1000
        guard width.isFinite, height.isFinite, scale.isFinite,
              width >= 1, height >= 1, scale >= 1,
              width <= CGFloat(UInt32.max), height <= CGFloat(UInt32.max), scale <= CGFloat(UInt32.max) else {
            if viewport != nil { clear(); viewport = nil; revision += 1; onViewport?(nil) }
            return
        }
        guard viewport == nil || frame != lastFrame || window !== lastWindow || zoom != lastZoom
                || deviceScale != lastDeviceScale || below.superview !== parent else { return }
        clear(); revision += 1; lastFrame = frame; lastWindow = window; lastZoom = zoom; lastDeviceScale = deviceScale
        parent.insertSubview(below, belowSubview: product); parent.insertSubview(above, aboveSubview: product)
        below.frame = frame; above.frame = frame
        viewport = NativeMediaViewport(revision: revision, width: UInt32(width),
            height: UInt32(height), deviceScaleNumerator: UInt32(scale),
            deviceScaleDenominator: 1000)
        onViewport?(viewport)
    }

    private func visibleInHierarchy(_ view: UIView) -> Bool {
        var ancestor: UIView? = view
        while let current = ancestor {
            if current.isHidden || current.alpha == 0 { return false }
            ancestor = current.superview
        }
        return true
    }

    func clear() {
        for picture in pictures {
            if let renderer = picture.renderer { picture.track?.remove(renderer) }
            picture.view.removeFromSuperview()
        }
        pictures.removeAll()
    }

    func resetSession() { clear(); layoutRevision = 0 }

    func removeParticipant(_ participantId: Data) {
        for index in pictures.indices.reversed() {
            guard case let .remote(id, _) = pictures[index].surface.source, id == participantId else { continue }
            let picture = pictures.remove(at: index)
            if let renderer = picture.renderer { picture.track?.remove(renderer) }
            picture.view.removeFromSuperview()
        }
    }

    func refresh(track: (NativeMediaPictureSource) throws -> RTCVideoTrack?) {
        CATransaction.begin(); CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        for index in pictures.indices {
            let replacement = try? track(pictures[index].surface.source)
            guard replacement !== pictures[index].track else { continue }
            if let renderer = pictures[index].renderer {
                pictures[index].track?.remove(renderer)
                renderer.removeFromSuperview()
            }
            // RTCMTLVideoView ignores renderFrame(nil). Releasing its renderer,
            // rather than sending nil, is what actually discards a stale frame.
            pictures[index].renderer = nil
            pictures[index].track = replacement
            if let replacement {
                let renderer = makeRenderer(surface: pictures[index].surface, view: pictures[index].view)
                pictures[index].renderer = renderer
                replacement.add(renderer)
            }
        }
    }

    private func makeRenderer(surface: NativeMediaSurface, view: UIView) -> RTCMTLVideoView {
        let renderer = RTCMTLVideoView(frame: CGRect(x: 0, y: 0, width: 1, height: 1))
        // Metal uses the source frame size for its drawable, not the potentially
        // huge logical layout rectangle. Bound the pre-first-frame drawable too.
        renderer.setSize(CGSize(width: 1, height: 1))
        renderer.frame = CGRect(x: CGFloat(surface.rect.x) * lastZoom - view.frame.minX,
            y: CGFloat(surface.rect.y) * lastZoom - view.frame.minY,
            width: CGFloat(surface.rect.width) * lastZoom, height: CGFloat(surface.rect.height) * lastZoom)
        renderer.videoContentMode = surface.fit == .cover ? .scaleAspectFill : .scaleAspectFit
        renderer.transform = surface.mirrored ? CGAffineTransform(scaleX: -1, y: 1) : .identity
        view.addSubview(renderer)
        return renderer
    }

    func set(viewportRevision: UInt64, layoutRevision: UInt64, surfaces: [NativeMediaSurface],
             track: (NativeMediaPictureSource) throws -> RTCVideoTrack?) throws {
        refreshViewport()
        if surfaces.isEmpty {
            guard layoutRevision > self.layoutRevision else {
                throw NativeMediaFailure.domain(.staleLayout(currentRevision: self.layoutRevision))
            }
            clear(); self.layoutRevision = layoutRevision; return
        }
        guard let viewport else { throw NativeMediaFailure.domain(.surfaceUnavailable) }
        guard viewportRevision == viewport.revision else {
            throw NativeMediaFailure.domain(.staleViewport(currentRevision: viewport.revision))
        }
        guard layoutRevision > self.layoutRevision else {
            throw NativeMediaFailure.domain(.staleLayout(currentRevision: self.layoutRevision))
        }
        guard surfaces.count <= 12, Set(surfaces.map(\.surfaceId)).count == surfaces.count else {
            throw NativeMediaFailure.domain(.invalidSurface)
        }
        var replacement: [Picture] = []
        for surface in surfaces.sorted(by: { ($0.depth, $0.surfaceId) < ($1.depth, $1.surfaceId) }) {
            let source = try track(surface.source)
            guard surface.visible else { continue }
            let scale = lastZoom
            func rect(_ value: NativeMediaRect) -> CGRect {
                CGRect(x: CGFloat(value.x) * scale, y: CGFloat(value.y) * scale,
                    width: CGFloat(value.width) * scale, height: CGFloat(value.height) * scale)
            }
            let target = rect(surface.rect)
            let clip = rect(surface.clip).intersection(target).intersection(below.bounds)
            guard !clip.isNull, !clip.isEmpty else { continue }
            let container = UIView(frame: clip)
            container.clipsToBounds = true
            container.isUserInteractionEnabled = false
            let mask = CAShapeLayer()
            mask.frame = container.bounds
            let radius = min(CGFloat(surface.cornerRadius) * scale, min(target.width, target.height) / 2)
            let rounded = UIBezierPath(roundedRect: target.offsetBy(dx: -clip.minX, dy: -clip.minY), cornerRadius: radius)
            mask.path = rounded.cgPath; container.layer.mask = mask
            let renderer = source.map { _ in makeRenderer(surface: surface, view: container) }
            container.tag = surface.placement == .belowProduct ? 0 : 1
            replacement.append(Picture(surface: surface, view: container, renderer: renderer, track: source))
        }
        // All validation/allocation precedes one main-runloop transaction. The
        // layers are siblings of WKWebView, never children of its snapshot tree.
        CATransaction.begin(); CATransaction.setDisableActions(true)
        clear()
        for picture in replacement {
            (picture.view.tag == 0 ? below : above).addSubview(picture.view)
            if let renderer = picture.renderer { picture.track?.add(renderer) }
        }
        pictures = replacement; self.layoutRevision = layoutRevision
        CATransaction.commit()
    }
}
