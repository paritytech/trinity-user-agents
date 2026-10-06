#if targetEnvironment(simulator) && E2E_TEST
    import Foundation
    import KeyDerivation
    import NovaCrypto
    import os
    import Products
    import UIKit
    import WebKit

    /// Runs the shared host-playground test list against the product inside this app, driven
    /// from outside by `e2e/host-playground/ios/run.mjs`.
    ///
    /// The runner places `seed`, `tests.json` and `page-runner.js` in `tmp/truapi-e2e/` of the
    /// data container and launches with `TRUAPI_IOS_E2E_HOST_PLAYGROUND=1`. The seed phrase is
    /// read and deleted before the root gates decide, so launch lands on the regular username
    /// check for the restored wallet. Once the tab bar is the root, the product opens and each
    /// test runs through the page runner while native confirmation sheets are answered
    /// in-process. `results.json` is rewritten after every test and `done` is written last.
    enum HostPlaygroundE2E {
        static let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("truapi-e2e", isDirectory: true)

        static let log = os.Logger(subsystem: "io.parity.polkadotapp.e2e", category: "host-playground")

        /// Seeds the wallet and theme, then starts the driver. Call before the root presenter is
        /// attached; does nothing unless the runner asked for it.
        static func install() {
            guard ProcessInfo.processInfo.environment["TRUAPI_IOS_E2E_HOST_PLAYGROUND"] == "1" else {
                return
            }

            var setupFailure: String?
            do {
                try restoreWallet()
                ThemeSelectionStorage().setSelected()
            } catch {
                setupFailure = "wallet setup failed: \(error)"
            }

            Task { @MainActor in
                await HostPlaygroundE2EDriver(setupFailure: setupFailure).run()
            }
        }

        private static func restoreWallet() throws {
            let seed = directory.appendingPathComponent("seed")
            let entropyManager = RootEntropyManager.shared

            guard FileManager.default.fileExists(atPath: seed.path) else {
                guard try entropyManager.hasRootEntropy() else {
                    throw HostPlaygroundE2EError("no seed file and no wallet on this install")
                }
                return
            }

            let phrase: String
            do {
                defer { try? FileManager.default.removeItem(at: seed) }
                phrase = try String(contentsOf: seed, encoding: .utf8)
            }

            let mnemonic = try IRMnemonicCreator().mnemonic(fromList: MnemonicTextNormalizer().process(text: phrase))

            if try entropyManager.hasRootEntropy() {
                guard try entropyManager.fetchRootEntropy() == mnemonic.entropy() else {
                    throw HostPlaygroundE2EError("another wallet is set up on this install; reinstall the app")
                }
                return
            }

            let walletSetupManager = WalletSetupManager(
                mnemonicGenerator: IRMnemonicCreator(),
                mnemonicBackupHelper: MnemonicBackupHelper(),
                entropyManager: entropyManager,
                logger: Logger.shared
            )
            try walletSetupManager.createWallets(with: AccountCreateMetadata(mnemonic: mnemonic))
            log.info("wallet restored from the runner's seed")
        }
    }

    struct HostPlaygroundE2EError: Error, CustomStringConvertible {
        let description: String

        init(_ description: String) {
            self.description = description
        }
    }

    /// The test list the runner copies from `e2e/host-playground/tests.json`.
    private struct HostPlaygroundTestList: Decodable {
        let product: String
        let hostPlaygroundCommit: String
        let tests: [String]
    }

    @MainActor
    private final class HostPlaygroundE2EDriver {
        private enum Timing {
            static let testTimeoutMs = 120_000
            static let tabBarTimeout: Duration = .seconds(600)
            static let productOpenTimeout: Duration = .seconds(180)
            static let productReopenInterval: Duration = .seconds(15)
            static let pageReadyTimeout: Duration = .seconds(120)
            static let foregroundTimeout: Duration = .seconds(120)
            static let poll: Duration = .seconds(1)
        }

        /// The world page-runner.js lives in. It shares the DOM with the product but none of its
        /// globals, so the product's own scripts cannot see or disturb the runner.
        private static let world = WKContentWorld.world(name: "host-playground-e2e")

        private let setupFailure: String?
        private let directory = HostPlaygroundE2E.directory
        private let log = HostPlaygroundE2E.log
        private let startedAt = ISO8601DateFormatter().string(from: Date())
        private let approver = HostPlaygroundE2EApprover()
        private var results: [[String: Any]] = []
        private var backgroundObserver: NSObjectProtocol?

        init(setupFailure: String?) {
            self.setupFailure = setupFailure
        }

        func run() async {
            defer { write(Data(), to: "done") }

            let list: HostPlaygroundTestList
            do {
                list = try JSONDecoder().decode(
                    HostPlaygroundTestList.self,
                    from: Data(contentsOf: directory.appendingPathComponent("tests.json"))
                )
            } catch {
                fail("cannot read tests.json: \(error)")
                return
            }

            observeBackgrounding()

            do {
                if let setupFailure {
                    throw HostPlaygroundE2EError(setupFailure)
                }

                let runner = try String(contentsOf: directory.appendingPathComponent("page-runner.js"), encoding: .utf8)
                try await waitForTabBar()
                let host = try await resolveHost(list.product)

                for id in list.tests {
                    log.info("running \(id, privacy: .public)")
                    let result = await runTest(id, host: host, runner: runner)
                    log.info("\(id, privacy: .public): \(String(describing: result["status"] ?? ""), privacy: .public)")
                    results.append(result)
                    writeResults(list)
                }
            } catch {
                fail("\(error)")
                let message = "not run: \(error)"
                for id in list.tests.dropFirst(results.count) {
                    results.append(["id": id, "status": "error", "message": message, "durationMs": 0])
                }
            }

            writeResults(list)
        }
    }

    // MARK: - Steps

    private extension HostPlaygroundE2EDriver {
        func waitForTabBar() async throws {
            try await poll(for: Timing.tabBarTimeout, "the main tab bar never became the root") {
                UIApplication.shared.mainTabBarController != nil
            }
            log.info("main tab bar is up")
        }

        func resolveHost(_ product: String) async throws -> ProductHost {
            guard let label = ProductHost.name(fromDotDomain: product) else {
                throw HostPlaygroundE2EError("unparseable product \(product)")
            }

            let factory = ProductHostFactory(tldProvider: DotNsTldProviderFacade.shared)
            guard let host = try await factory.resolveHost(label: label) else {
                throw HostPlaygroundE2EError("no product host for \(label)")
            }
            log.info("product is \(host.toDotDomain(), privacy: .public)")
            return host
        }

        func runTest(_ id: String, host: ProductHost, runner: String) async -> [String: Any] {
            let started = ContinuousClock.now
            do {
                try await waitUntilActive()
                let webView = try await openProduct(host)
                try await ensureRunner(in: webView, source: runner)

                approver.start()
                defer { approver.stop() }

                let value = try await evaluate(
                    "return await window.__hostPlaygroundE2E.runOne(id, timeoutMs);",
                    in: webView,
                    arguments: ["id": id, "timeoutMs": Timing.testTimeoutMs],
                    timeout: .milliseconds(Timing.testTimeoutMs) + .seconds(60)
                )
                guard let result = value as? [String: Any] else {
                    throw HostPlaygroundE2EError("runOne returned \(String(describing: value))")
                }
                return result.filter { ["id", "status", "outcome", "message", "durationMs"].contains($0.key) }
            } catch {
                let elapsed = ContinuousClock.now - started
                return [
                    "id": id,
                    "status": "error",
                    "message": "driver: \(error)",
                    "durationMs": elapsed.components.seconds * 1000
                ]
            }
        }

        /// The product's web view, opening the product again when it is not on screen: a test
        /// can navigate away, open another product or minimize this one.
        func openProduct(_ host: ProductHost) async throws -> WKWebView {
            let domain = host.toDotDomain()
            let deadline = ContinuousClock.now + Timing.productOpenTimeout
            var lastOpen: ContinuousClock.Instant?

            while ContinuousClock.now < deadline {
                if let webView = visibleWebView(host: domain) {
                    return webView
                }
                if lastOpen.map({ ContinuousClock.now - $0 > Timing.productReopenInterval }) ?? true {
                    log.info("opening \(domain, privacy: .public)")
                    ModuleNavigator().openProduct(page: ProductPage(host: host))
                    lastOpen = .now
                }
                try await Task.sleep(for: Timing.poll)
            }
            throw HostPlaygroundE2EError("\(domain) did not open")
        }

        /// Injects page-runner.js when the page does not have it, which is after every reload,
        /// and waits for the playground to render its run buttons. Both happen in one evaluation,
        /// retried, because the product can reload while it settles and drop the runner in between.
        func ensureRunner(in webView: WKWebView, source: String) async throws {
            let deadline = ContinuousClock.now + Timing.pageReadyTimeout
            while ContinuousClock.now < deadline {
                let ready = try? await evaluate(
                    source + "\nreturn window.__hostPlaygroundE2E.ready();",
                    in: webView
                ) as? Bool
                if ready == true {
                    return
                }
                try await Task.sleep(for: Timing.poll)
            }
            throw HostPlaygroundE2EError("the playground rendered no run buttons")
        }

        /// A test that opens an external URL sends the app to the background, where its web
        /// views stop running script; the runner brings it back when `backgrounded` appears.
        /// Waits out a trip to the background. A system alert, such as the notification prompt the
        /// simulator cannot pre-grant, leaves the app inactive rather than backgrounded, and the page
        /// keeps running underneath it, so inactive counts as foreground.
        func waitUntilActive() async throws {
            try await poll(for: Timing.foregroundTimeout, "the app did not return to the foreground") {
                UIApplication.shared.applicationState != .background
            }
        }

        func observeBackgrounding() {
            backgroundObserver = NotificationCenter.default.addObserver(
                forName: UIApplication.didEnterBackgroundNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    self?.log.info("app went to the background")
                    self?.write(Data(), to: "backgrounded")
                }
            }
        }
    }

    // MARK: - Helpers

    private extension HostPlaygroundE2EDriver {
        func visibleWebView(host: String) -> WKWebView? {
            UIApplication.shared.connectedScenes
                .compactMap { $0 as? UIWindowScene }
                .flatMap(\.windows)
                .lazy
                .compactMap { Self.webView(in: $0, host: host) }
                .first
        }

        static func webView(in view: UIView, host: String) -> WKWebView? {
            if let webView = view as? WKWebView {
                return webView.url?.host == host && !webView.isHidden ? webView : nil
            }
            return view.subviews.lazy.compactMap { webView(in: $0, host: host) }.first
        }

        func evaluate(
            _ body: String,
            in webView: WKWebView,
            arguments: [String: Any] = [:],
            timeout: Duration = .seconds(30)
        ) async throws -> Any? {
            try await withCheckedThrowingContinuation { continuation in
                var finished = false
                let finish: (Result<Any?, Error>) -> Void = { result in
                    guard !finished else { return }
                    finished = true
                    continuation.resume(with: result)
                }

                webView.callAsyncJavaScript(body, arguments: arguments, in: nil, in: Self.world) { result in
                    finish(result.map { Optional($0) })
                }

                Task { @MainActor in
                    try? await Task.sleep(for: timeout)
                    finish(.failure(HostPlaygroundE2EError("no answer from the page within \(timeout)")))
                }
            }
        }

        func poll(for timeout: Duration, _ failure: String, until condition: () -> Bool) async throws {
            let deadline = ContinuousClock.now + timeout
            while !condition() {
                guard ContinuousClock.now < deadline else {
                    throw HostPlaygroundE2EError(failure)
                }
                try await Task.sleep(for: Timing.poll)
            }
        }

        func fail(_ message: String) {
            log.error("\(message, privacy: .public)")
            write(Data(message.utf8), to: "failure")
        }

        func writeResults(_ list: HostPlaygroundTestList) {
            let info = Bundle.main.infoDictionary
            let version = info?["CFBundleShortVersionString"] as? String ?? "?"
            let build = info?["CFBundleVersion"] as? String ?? "?"
            let run: [String: Any] = [
                "platform": "ios",
                "app": "\(Bundle.main.bundleIdentifier ?? "polkadot-app") \(version) (\(build))",
                "product": list.product,
                "hostPlaygroundCommit": list.hostPlaygroundCommit,
                "startedAt": startedAt,
                "results": results
            ]

            do {
                let data = try JSONSerialization.data(withJSONObject: run, options: [.prettyPrinted, .sortedKeys])
                write(data, to: "results.json")
            } catch {
                log.error("cannot encode results: \(error, privacy: .public)")
            }
        }

        func write(_ data: Data, to name: String) {
            do {
                try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
                try data.write(to: directory.appendingPathComponent(name), options: .atomic)
            } catch {
                log.error("cannot write \(name, privacy: .public): \(error, privacy: .public)")
            }
        }
    }
#endif
