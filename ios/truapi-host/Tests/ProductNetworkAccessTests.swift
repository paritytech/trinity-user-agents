#if canImport(UIKit) && canImport(WebKit)

import Foundation
import Network
import Testing
import UIKit
import WebKit
@testable import TrUAPIHost

@Suite(.serialized)
@MainActor
struct ProductNetworkAccessTests {
    @Test(.timeLimit(.minutes(1)))
    func retainedClientAndFetchRecoverWithoutLifecycleCallbacks() async throws {
        let product = try await NetworkTestProduct.open(initialScripts: ["""
            const NativeSocket = WebSocket;
            const closeSocket = NativeSocket.prototype.close;
            window.WebSocket = new Proxy(NativeSocket, {
              construct(target, args) {
                const socket = Reflect.construct(target, args);
                window.__testDisconnectHost = () => closeSocket.call(socket);
                return socket;
              }
            });
            """])
        defer { product.close() }
        try product.execution.setPermissionAuthorizationStatus(
            request: .remote(RemotePermissionRequest(permission: .remote(domains: ["127.0.0.1"]))),
            status: .authorized
        )
        let remote = product.server.url(host: "127.0.0.1", path: "/allowed")
        let result = try await withNetworkTestTimeout("retained client recovery") {
            try await product.webView.callAsyncJavaScript("""
                const host = window.__HOST_API_CLIENT__;
                const client = host.client;
                const statuses = [];
                host.subscribeConnectionStatus(status => statuses.push(status));
                (await client.system.handshake())._unsafeUnwrap();
                const results = [];
                let interrupted = 0;
                for (let cycle = 0; cycle < 2; cycle++) {
                  client.theme.subscribe().subscribe({ error() { interrupted++; } });
                  const disconnected = new Promise(resolve => {
                    const unsubscribe = host.subscribeConnectionStatus(status => {
                      if (status === 'disconnected') { unsubscribe(); resolve(); }
                    });
                  });
                  window.__testDisconnectHost();
                  await disconnected;
                  (await client.system.handshake())._unsafeUnwrap();
                  results.push(await (await fetch(url)).text());
                }
                return JSON.stringify({
                  sameClient: host.client === client,
                  interrupted,
                  resets: statuses.filter(status => status === 'disconnected').length,
                  results,
                });
                """, arguments: ["url": remote.absoluteString], in: nil, contentWorld: .page) as? String
        }
        #expect(result == """
            {"sameClient":true,"interrupted":2,"resets":2,"results":["allowed","allowed"]}
            """)
        #expect(product.server.requests(path: "/allowed") == 2)
    }

    @Test(.timeLimit(.minutes(1)))
    func fetchUsesRustPermissionsAndPreservesNativeRedirects() async throws {
        let product = try await NetworkTestProduct.open()
        defer { product.close() }
        let remote = product.server.url(host: "127.0.0.1", path: "/allowed")
        #expect(try await fetch(product.webView, remote) == "denied")
        #expect(product.server.requests(path: "/allowed") == 0)

        let permission = PermissionAuthorizationRequest.remote(
            RemotePermissionRequest(permission: .remote(domains: ["127.0.0.1"]))
        )
        try product.execution.setPermissionAuthorizationStatus(request: permission, status: .authorized)
        #expect(try await fetch(product.webView, remote) == "allowed")
        #expect(product.server.requests(path: "/allowed") == 1)

        let redirect = product.server.url(host: "127.0.0.1", path: "/redirect-denied")
        #expect(try await fetch(product.webView, redirect) == "allowed")
        #expect(product.server.requests(path: "/blocked") == 1)

        try product.execution.setPermissionAuthorizationStatus(request: permission, status: .denied)
        #expect(try await fetch(product.webView, remote) == "denied")
        #expect(product.server.requests(path: "/allowed") == 1)
    }

    @Test(.timeLimit(.minutes(1)))
    func allowOnceAuthorizesOnlyTheNextFetch() async throws {
        let product = try await NetworkTestProduct.open(
            bridge: StubHostBridge(remoteDecisions: [.allowOnce, .deny])
        )
        defer { product.close() }
        let remote = product.server.url(host: "127.0.0.1", path: "/allowed")
        #expect(try await fetch(product.webView, remote) == "allowed")
        #expect(try await fetch(product.webView, remote) == "denied")
        #expect(product.server.requests(path: "/allowed") == 1)
    }

    @Test(.timeLimit(.minutes(1)))
    func allowOnceAuthorizesOneWebRtcConnection() async throws {
        let product = try await NetworkTestProduct.open(
            bridge: StubHostBridge(remoteDecisions: [.allowOnce, .deny])
        )
        defer { product.close() }

        let decisions = try await withNetworkTestTimeout("WebRTC permission") {
            try await product.webView.callAsyncJavaScript("""
                const first = new RTCPeerConnection({ iceServers: [] });
                const second = new RTCPeerConnection({ iceServers: [] });
                try {
                  const offers = [await first.createOffer(), await first.createOffer()];
                  let secondDecision = 'allowed';
                  try { await second.createOffer(); } catch { secondDecision = 'denied'; }
                  return [...offers.map(offer => offer.type), secondDecision];
                } finally {
                  first.close();
                  second.close();
                }
                """, arguments: [:], in: nil, contentWorld: .page) as? [String]
        }

        #expect(decisions == ["offer", "offer", "denied"])
    }

    @Test(.timeLimit(.minutes(1)))
    func allowOnceReachesStubbedMediaCaptureOnlyOnce() async throws {
        let bridge = StubHostBridge(deviceDecisions: [.allowOnce, .allowOnce, .deny])
        let product = try await NetworkTestProduct.open(bridge: bridge, initialScripts: ["""
            window.__testMediaCalls = [];
            Object.defineProperty(Object.getPrototypeOf(navigator.mediaDevices), 'getUserMedia', {
              configurable: true,
              writable: true,
              value: async function(constraints) {
                window.__testMediaCalls.push({ audio: !!constraints.audio, video: !!constraints.video });
                return { getTracks: () => [] };
              }
            });
            """])
        defer { product.close() }

        let result = try await withNetworkTestTimeout("media permission") {
            try await product.webView.callAsyncJavaScript("""
                if (typeof navigator.mediaDevices?.getUserMedia !== 'function') {
                  throw new Error('Media capture API is not exposed');
                }
                const decisions = [];
                for (let attempt = 0; attempt < 2; attempt++) {
                  try {
                    await navigator.mediaDevices.getUserMedia({ audio: true, video: true });
                    decisions.push('allowed');
                  } catch (error) {
                    if (!(error instanceof DOMException) || error.name !== 'NotAllowedError') throw error;
                    decisions.push('denied');
                  }
                }
                return JSON.stringify({ decisions, captures: window.__testMediaCalls });
                """, arguments: [:], in: nil, contentWorld: .page) as? String
        }

        #expect(result == """
            {"decisions":["allowed","denied"],"captures":[{"audio":true,"video":true}]}
            """)
        #expect(bridge.requestedDevicePermissions == [.camera, .microphone, .camera])
    }

    @Test(.timeLimit(.minutes(1)))
    func stylesheetsAndFontsKeepTheirNativeLoadingBehavior() async throws {
        let product = try await NetworkTestProduct.open()
        defer { product.close() }
        let stylesheet = product.server.url(host: "127.0.0.1", path: "/style.css")
        let font = product.server.url(host: "127.0.0.1", path: "/font.woff2")
        let loaded = try await withNetworkTestTimeout("stylesheet and font") {
            try await product.webView.callAsyncJavaScript("""
                const stylesheetLoaded = await new Promise(resolve => {
                  const link = document.createElement('link');
                  link.rel = 'stylesheet'; link.href = stylesheet;
                  link.onload = () => resolve(true); link.onerror = () => resolve(false);
                  document.head.appendChild(link);
                });
                try { await new FontFace('test-font', `url(${font})`).load(); } catch {}
                return stylesheetLoaded;
                """, arguments: ["stylesheet": stylesheet.absoluteString, "font": font.absoluteString],
                in: nil, contentWorld: .page) as? Bool
        }
        #expect(loaded == true)
        #expect(product.server.requests(path: "/style.css") == 1)
        #expect(product.server.requests(path: "/font.woff2") == 1)
    }

    private func fetch(_ webView: WKWebView, _ url: URL) async throws -> String {
        try await withNetworkTestTimeout("fetch \(url.absoluteString)") {
            try await webView.callAsyncJavaScript(
                "try { const response = await fetch(url); return await response.text(); } catch { return 'denied'; }",
                arguments: ["url": url.absoluteString], in: nil, contentWorld: .page
            ) as? String ?? "evaluation failed"
        }
    }
}

private struct NetworkTestTimeout: Error, CustomStringConvertible {
    let stage: String
    var description: String { "Timed out waiting for \(stage)" }
}

@MainActor
private func withNetworkTestTimeout<Value: Sendable>(
    _ stage: String, operation: @escaping @MainActor () async throws -> Value
) async throws -> Value {
    let result = AsyncThrowingStream<Value, Error>.makeStream()
    let timeout = Task {
        try await Task.sleep(for: .seconds(15))
        result.continuation.finish(throwing: NetworkTestTimeout(stage: stage))
    }
    let task = Task {
        do {
            result.continuation.yield(try await operation())
            result.continuation.finish()
        } catch {
            result.continuation.finish(throwing: error)
        }
    }
    defer {
        timeout.cancel()
        task.cancel()
    }
    var iterator = result.stream.makeAsyncIterator()
    guard let value = try await iterator.next() else { throw CancellationError() }
    return value
}

@MainActor
private final class NetworkTestWindow {
    private let window: UIWindow

    init(_ webView: WKWebView) throws {
        let scene = try #require(UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
            .first { $0.activationState == .foregroundActive }, "Run WebKit tests in NetworkTestHost")
        window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 320, height: 480)
        let controller = UIViewController()
        controller.view = webView
        window.rootViewController = controller
        window.makeKeyAndVisible()
        #expect(webView.window === window)
    }

    func close() {
        window.isHidden = true
        window.rootViewController = nil
        window.windowScene = nil
    }
}

@MainActor
private struct NetworkTestProduct {
    let server: NetworkTestServer
    let execution: TrUAPIProductExecution
    let webView: WKWebView
    let window: NetworkTestWindow
    let navigationDelegate: ProductPageReady

    static func open(
        bridge: StubHostBridge = StubHostBridge(),
        initialScripts: [String] = []
    ) async throws -> NetworkTestProduct {
        let server = try await NetworkTestServer.start()
        do {
            let runtime = try TrUAPIHostRuntime(bridge: bridge, runtimeConfig: HostRuntimeConfig(
                hostName: "network-tests", peopleChainGenesisHash: Data(repeating: 0, count: 32),
                bulletinChainGenesisHash: Data(repeating: 0, count: 32),
                assetHubChainGenesisHash: Data(repeating: 1, count: 32), networkSuffix: "paseo",
                databaseDirectory: temporaryDatabaseDirectory()
            ))
            let execution = try runtime.openProductExecution(
                bridge: bridge,
                configuration: ProductExecutionConfig(productId: "network.paseo", executionKind: .app)
            )
            let ready = ProductPageReady()
            let configuration = WKWebViewConfiguration()
            // Local fixtures must not wait for Safari's Safe Browsing database.
            configuration.preferences.isFraudulentWebsiteWarningEnabled = false
            configuration.userContentController.add(ready, name: "testReady")
            for source in initialScripts {
                configuration.userContentController.addUserScript(WKUserScript(
                    source: source, injectionTime: .atDocumentStart, forMainFrameOnly: true
                ))
            }
            let webView = WKWebView(frame: .zero, configuration: configuration)
            webView.navigationDelegate = ready
            let window = try NetworkTestWindow(webView)
            do {
                try TrUAPIHost.installProductScripts(
                    into: webView, endpoint: execution.startWsBridge(bindPort: 0)
                )
                #expect(webView.navigationDelegate === ready)
                #expect(webView.configuration.websiteDataStore.isPersistent)
                try await ready.load(webView, url: server.url(host: "localhost", path: "/product"))
                return NetworkTestProduct(
                    server: server, execution: execution, webView: webView, window: window, navigationDelegate: ready
                )
            } catch {
                window.close()
                execution.close()
                throw error
            }
        } catch {
            server.stop()
            throw error
        }
    }

    func close() {
        webView.stopLoading()
        window.close()
        execution.close()
        server.stop()
    }
}

@MainActor
private final class ProductPageReady: NSObject, WKScriptMessageHandler, WKNavigationDelegate {
    private var onReady: (() -> Void)?

    func load(_ webView: WKWebView, url: URL) async throws {
        let ready = AsyncStream<Void>.makeStream()
        onReady = {
            ready.continuation.yield(())
            ready.continuation.finish()
        }
        defer { onReady = nil }
        webView.load(URLRequest(url: url))
        try await withNetworkTestTimeout("page ready \(url.absoluteString)") {
            var iterator = ready.stream.makeAsyncIterator()
            guard await iterator.next() != nil else { throw CancellationError() }
        }
    }

    func userContentController(_: WKUserContentController, didReceive _: WKScriptMessage) {
        let callback = onReady
        onReady = nil
        callback?()
    }
}

private final class NetworkTestServer: @unchecked Sendable {
    private let listener: NWListener
    private let lock = NSLock()
    private var counts: [String: Int] = [:]

    private init(listener: NWListener) { self.listener = listener }

    @MainActor
    static func start() async throws -> NetworkTestServer {
        let server = NetworkTestServer(listener: try NWListener(using: .tcp, on: .any))
        server.listener.newConnectionHandler = { connection in server.accept(connection) }
        let ready = AsyncThrowingStream<Void, Error>.makeStream()
        server.listener.stateUpdateHandler = { state in
            switch state {
            case .ready:
                ready.continuation.yield(())
                ready.continuation.finish()
            case let .failed(error):
                ready.continuation.finish(throwing: error)
            default: break
            }
        }
        server.listener.start(queue: DispatchQueue(label: "network-test-server"))
        do {
            try await withNetworkTestTimeout("loopback listener ready") {
                var iterator = ready.stream.makeAsyncIterator()
                guard try await iterator.next() != nil else { throw CancellationError() }
            }
        } catch {
            server.stop()
            throw error
        }
        server.listener.stateUpdateHandler = nil
        return server
    }

    func stop() {
        listener.newConnectionHandler = nil
        listener.cancel()
    }

    func url(host: String, path: String) -> URL {
        URL(string: "http://\(host):\(listener.port!.rawValue)\(path)")!
    }

    func requests(path: String) -> Int {
        lock.withLock { counts[path, default: 0] }
    }

    private func accept(_ connection: NWConnection) {
        connection.start(queue: DispatchQueue(label: "network-test-connection"))
        receiveRequest(connection, previous: Data())
    }

    private func receiveRequest(_ connection: NWConnection, previous: Data) {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 65536) { data, _, _, _ in
            guard let data, previous.count + data.count <= 65536 else {
                connection.cancel()
                return
            }
            let buffer = previous + data
            guard buffer.range(of: Data("\r\n\r\n".utf8)) != nil else {
                self.receiveRequest(connection, previous: buffer)
                return
            }
            guard let request = String(data: buffer, encoding: .utf8),
                  let path = request.split(separator: " ").dropFirst().first else {
                connection.cancel()
                return
            }
            self.lock.withLock { self.counts[String(path), default: 0] += 1 }
            var status = "200 OK"
            var headers = "Access-Control-Allow-Origin: *\r\nContent-Type: text/html\r\nCache-Control: no-store\r\n"
            var body = "allowed"
            if path == "/product" {
                body = "<script>window.webkit.messageHandlers.testReady.postMessage('ready')</script>"
            } else if path == "/style.css" {
                headers = "Access-Control-Allow-Origin: *\r\nContent-Type: text/css\r\nCache-Control: no-store\r\n"
                body = "body { color: green; }"
            } else if path == "/redirect-denied" {
                status = "302 Found"
                let destination = self.url(host: "[::1]", path: "/blocked")
                headers += "Location: \(destination.absoluteString)\r\n"
                body = ""
            }
            let response = "HTTP/1.1 \(status)\r\n\(headers)Content-Length: \(body.utf8.count)\r\nConnection: close\r\n\r\n\(body)"
            connection.send(content: Data(response.utf8), completion: .contentProcessed { _ in connection.cancel() })
        }
    }
}

#endif
