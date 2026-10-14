import Foundation
import Products

extension JSEngineScript {
    /// Pins the viewport so product pages cannot zoom; shared by the native
    /// and rust SPA scripts factories.
    static let disableZoom = JSEngineScript(
        content: """
        var meta = document.createElement('meta');
        meta.name = 'viewport';
        meta.content = 'width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no';
        document.head.appendChild(meta);
        """,
        insertionPoint: .atDocEnd
    )
}
