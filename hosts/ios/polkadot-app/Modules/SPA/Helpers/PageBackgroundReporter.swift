import Foundation
import Products

/// Reports the computed backgrounds of the page's root element and body whenever they change.
///
/// A page can restyle after it loads, for example once it receives the host theme, so a single read at load
/// time goes stale. The script watches the document and posts the colours, outermost first, on the frame after
/// a change, and only when they differ from the last report.
final class PageBackgroundReporter {
    private let onChange: ([String]) -> Void

    init(onChange: @escaping ([String]) -> Void) {
        self.onChange = onChange
    }
}

extension PageBackgroundReporter: JSEngineScriptHandling {
    func getScript() -> JSEngineScript {
        JSEngineScript(content: Self.script, insertionPoint: .atDocStart)
    }

    var handlerName: String {
        Self.handlerName
    }

    func handle(body: Any) {
        guard let cssColors = body as? [String] else { return }

        onChange(cssColors)
    }
}

private extension PageBackgroundReporter {
    static let handlerName = "__pageBackground__"

    static let script = """
    (function() {
        var lastReport = null;
        var isScheduled = false;

        function report() {
            isScheduled = false;
            var colors = [
                getComputedStyle(document.documentElement).backgroundColor,
                document.body ? getComputedStyle(document.body).backgroundColor : 'rgba(0, 0, 0, 0)'
            ];
            var key = colors.join('|');
            if (key === lastReport) { return; }
            lastReport = key;
            window.webkit.messageHandlers.\(handlerName).postMessage(colors);
        }

        function schedule() {
            if (isScheduled) { return; }
            isScheduled = true;
            requestAnimationFrame(report);
        }

        new MutationObserver(schedule).observe(document.documentElement, {
            attributes: true,
            childList: true,
            subtree: true
        });
        matchMedia('(prefers-color-scheme: dark)').addEventListener('change', schedule);
        document.addEventListener('transitionend', schedule, true);
        schedule();
    })();
    """
}
