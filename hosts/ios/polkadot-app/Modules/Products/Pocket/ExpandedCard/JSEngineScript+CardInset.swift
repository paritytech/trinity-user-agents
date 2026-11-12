import CoreGraphics
import Products

extension JSEngineScript {
    /// Sets `--pocket-card-inset`, where a page under a Pocket card starts its content.
    static func pocketCardInset(_ inset: CGFloat) -> JSEngineScript {
        JSEngineScript(
            content: "document.documentElement.style.setProperty('--pocket-card-inset', '\(inset)px');",
            insertionPoint: .atDocStart
        )
    }
}
