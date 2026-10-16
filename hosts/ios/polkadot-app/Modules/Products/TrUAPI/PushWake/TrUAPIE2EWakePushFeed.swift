import AVFoundation
import Foundation
import UIKit

#if targetEnvironment(simulator)
    /// The wake pushes of the truapi E2E harness (`TRUAPI_IOS_E2E_WAKE_PUSH_FEED=1`, VERBOSITY-WASM-PRODUCT A1).
    ///
    /// A simulator cannot get a background push: `xcrun simctl push` refuses a payload with no visible content, and
    /// it does not call the app delegate. So the reference relay (`truapi-host wake-relay --sink simulator`) puts each
    /// push payload as a JSON file in `tmp/truapi-e2e/wake-pushes` and posts the Darwin notification
    /// ``notificationName``. The feed then gives each payload to the app delegate, as iOS does for a background push.
    ///
    /// iOS suspends an app in the background, and a suspended app gets no Darwin notification. So while the feed is
    /// on, the app plays silence in the background and is not suspended. This replaces the step where iOS resumes the
    /// app for a push; everything after that step is the path of a real push.
    @MainActor
    enum TrUAPIE2EWakePushFeed {
        static let notificationName = "io.parity.truapi.e2e.wake-push"

        static let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("truapi-e2e/wake-pushes", isDirectory: true)

        static var isEnabled: Bool {
            ProcessInfo.processInfo.environment["TRUAPI_IOS_E2E_WAKE_PUSH_FEED"] == "1"
        }

        private static var stayAwake: TrUAPIE2EStayAwake?

        /// Starts the feed when the harness asks for it.
        static func start(delegate: UIApplicationDelegate, logger: LoggerProtocol = Logger.shared) {
            guard isEnabled else { return }

            try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            stayAwake = TrUAPIE2EStayAwake(logger: logger)
            let observer = Unmanaged.passUnretained(delegate as AnyObject).toOpaque()
            CFNotificationCenterAddObserver(
                CFNotificationCenterGetDarwinNotifyCenter(),
                observer,
                wakePushPosted,
                notificationName as CFString,
                nil,
                .deliverImmediately
            )
            logger.info("[e2e] wake push feed on")
        }

        /// Gives each queued payload to the app delegate, in name order, and removes its file.
        static func deliver(to delegate: UIApplicationDelegate, logger: LoggerProtocol = Logger.shared) {
            let fileManager = FileManager.default
            let files = ((try? fileManager.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)) ?? [])
                .filter { $0.pathExtension == "json" }
                .sorted { $0.lastPathComponent < $1.lastPathComponent }
            for file in files {
                defer { try? fileManager.removeItem(at: file) }
                guard
                    let data = try? Data(contentsOf: file),
                    let userInfo = try? JSONSerialization.jsonObject(with: data) as? [AnyHashable: Any]
                else {
                    logger.warning("[e2e] wake push \(file.lastPathComponent) is not a push payload")
                    continue
                }
                logger.info("[e2e] wake push \(file.lastPathComponent) delivered to the app")
                delegate.application?(UIApplication.shared, didReceiveRemoteNotification: userInfo) { result in
                    logger.info("[e2e] wake push \(file.lastPathComponent) ended: \(result.rawValue)")
                }
            }
        }
    }

    /// Delivers the queued payloads when the relay posts the Darwin notification of the feed.
    private let wakePushPosted: CFNotificationCallback = { _, observer, _, _, _ in
        guard let observer else { return }
        let delegate = Unmanaged<AnyObject>.fromOpaque(observer).takeUnretainedValue()
        DispatchQueue.main.async {
            MainActor.assumeIsolated {
                guard let delegate = delegate as? UIApplicationDelegate else { return }
                TrUAPIE2EWakePushFeed.deliver(to: delegate)
            }
        }
    }

    /// Plays silence while the app is in the background, so that the simulator does not suspend the app.
    @MainActor
    final class TrUAPIE2EStayAwake {
        private let engine = AVAudioEngine()
        private var observers: [NSObjectProtocol] = []
        private let logger: LoggerProtocol

        init(logger: LoggerProtocol) {
            self.logger = logger
            let silence = AVAudioSourceNode { _, _, _, buffers in
                for buffer in UnsafeMutableAudioBufferListPointer(buffers) {
                    if let data = buffer.mData {
                        memset(data, 0, Int(buffer.mDataByteSize))
                    }
                }
                return noErr
            }
            engine.attach(silence)
            engine.connect(silence, to: engine.mainMixerNode, format: nil)
            let center = NotificationCenter.default
            observers = [
                center
                    .addObserver(
                        forName: UIApplication.didEnterBackgroundNotification,
                        object: nil,
                        queue: .main
                    ) { _ in
                        MainActor.assumeIsolated { self.play() }
                    },
                center
                    .addObserver(
                        forName: UIApplication.willEnterForegroundNotification,
                        object: nil,
                        queue: .main
                    ) { _ in
                        MainActor.assumeIsolated { self.engine.pause() }
                    }
            ]
        }

        private func play() {
            do {
                try AVAudioSession.sharedInstance().setCategory(.playback, options: [.mixWithOthers])
                try AVAudioSession.sharedInstance().setActive(true)
                try engine.start()
                logger.info("[e2e] the app stays awake in the background")
            } catch {
                logger.warning("[e2e] the app cannot stay awake in the background: \(error)")
            }
        }
    }
#endif
