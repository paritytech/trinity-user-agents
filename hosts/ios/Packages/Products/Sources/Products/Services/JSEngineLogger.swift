import Foundation
import SDKLogger

public final class JSEngineLogger {
    let logger: SDKLoggerProtocol

    public init(logger: SDKLoggerProtocol) {
        self.logger = logger
    }
}

extension JSEngineLogger: JSEngineScriptHandling {
    public func getScript() -> JSEngineScript {
        JSEngineScript(
            content: Self.consoleOverrideScript(),
            insertionPoint: .atDocStart
        )
    }

    public var handlerName: String {
        Self.handlerName
    }

    public func handle(body: Any) {
        guard let dict = body as? [String: Any],
              let level = dict["level"] as? String,
              let message = dict["message"] as? String else { return }

        switch level {
        case "error":
            logger.error("JS: \(message)")
        case "warn":
            logger.warning("JS: \(message)")
        default:
            logger.debug("JS: \(message)")
        }
    }
}

private extension JSEngineLogger {
    static let handlerName = "__console__"

    static func consoleOverrideScript() -> String {
        """
        (function() {
            var origLog = console.log;
            var origWarn = console.warn;
            var origError = console.error;

            function send(level, args) {
                var msg = Array.prototype.slice.call(args).map(function(a) {
                    try { return typeof a === 'object' ? JSON.stringify(a) : String(a); }
                    catch(e) { return String(a); }
                }).join(' ');
                window.webkit.messageHandlers.\(handlerName).postMessage({ level: level, message: msg });
            }

            console.log = function() { send('log', arguments); origLog.apply(console, arguments); };
            console.warn = function() { send('warn', arguments); origWarn.apply(console, arguments); };
            console.error = function() { send('error', arguments); origError.apply(console, arguments); };

            window.addEventListener('error', function(e) {
                send('error', ['Uncaught error: ' + e.message + ' at ' + e.filename + ':' + e.lineno]);
            });

            window.addEventListener('unhandledrejection', function(e) {
                send('error', ['Unhandled promise rejection: ' + e.reason]);
            });
        })();
        """
    }
}
