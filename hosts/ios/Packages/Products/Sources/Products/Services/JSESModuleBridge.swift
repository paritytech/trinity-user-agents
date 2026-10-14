import Foundation

/// Manages ES module script loading via `<script type="module">` injection.
///
/// Registers `__module_complete__` and `__module_error__` handlers on the engine,
/// then uses `engine.evaluate()` to inject module script tags. Tracks pending loads
/// via checked continuations keyed by a unique module ID.
public actor JSESModuleBridge {
    private let engine: JSEngineProtocol
    private var continuations: [String: CheckedContinuation<Void, Error>] = [:]

    public init(engine: JSEngineProtocol) {
        self.engine = engine
    }

    /// Register the `__module_complete__` and `__module_error__` handlers on the engine.
    /// Must be called before executing any module scripts.
    public func install() async {
        await engine.registerFunction(name: "__module_complete__") { [weak self] args in
            await self?.handleComplete(moduleId: args)
        }
        await engine.registerFunction(name: "__module_error__") { [weak self] args in
            await self?.handleError(moduleId: args)
        }
    }

    /// Load an ES module script by injecting a `<script type="module">` tag.
    public func executeScript(url: URL) async throws {
        let moduleId = UUID().uuidString
        let escapedURL = url.absoluteString.jsEscaped

        let js = """
        (function() {
            var s = document.createElement('script');
            s.type = 'module';
            s.src = '\(escapedURL)';
            s.onload = function() {
                window.webkit.messageHandlers.__module_complete__.postMessage('\(moduleId)');
            };
            s.onerror = function() {
                window.webkit.messageHandlers.__module_error__.postMessage('\(moduleId)');
            };
            document.body.appendChild(s);
        })();
        """

        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            continuations[moduleId] = continuation

            Task { [weak self] in
                do {
                    try await self?.engine.evaluate(js)
                } catch {
                    await self?.failModule(moduleId: moduleId, error: error)
                }
            }
        }
    }

    /// Cancel all pending module loads and release continuations.
    public func dispose() {
        for (_, continuation) in continuations {
            continuation.resume(throwing: JSEngineError.moduleLoadFailed)
        }
        continuations.removeAll()
    }

    // MARK: - Private

    private func handleComplete(moduleId: String) {
        continuations.removeValue(forKey: moduleId)?.resume()
    }

    private func handleError(moduleId: String) {
        continuations.removeValue(forKey: moduleId)?.resume(throwing: JSEngineError.moduleLoadFailed)
    }

    private func failModule(moduleId: String, error: Error) {
        continuations.removeValue(forKey: moduleId)?.resume(throwing: error)
    }
}
