import Foundation
import Products

extension JSEngineScript {
    /// Pins the viewport so product pages cannot zoom; shared by the native
    /// and rust SPA scripts factories. The page's own `viewport-fit` is kept, since the last
    /// viewport tag wins and without `cover` every `env(safe-area-inset-*)` reads 0.
    static let disableZoom = JSEngineScript(
        content: """
        var pageViewport = document.querySelector('meta[name="viewport"]');
        var fit = pageViewport && /viewport-fit\\s*=\\s*([a-z]+)/i.exec(pageViewport.content);
        var meta = document.createElement('meta');
        meta.name = 'viewport';
        meta.content = 'width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no'
            + (fit ? ', viewport-fit=' + fit[1] : '');
        document.head.appendChild(meta);
        """,
        insertionPoint: .atDocEnd
    )
}
