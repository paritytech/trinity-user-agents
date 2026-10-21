import SwiftUI
import UIKit

/// The faces of the inline styles that the design-system family does not have. The family has no italic and no
/// monospaced face, so `Text` draws an emphasized or a code run in the regular face. Such a run gets the system
/// face at the size of the text.
public enum ChatTextFaces {
    /// The size of an inline code run, relative to the size of the text. A monospaced face looks larger.
    static let codeScale: CGFloat = 0.94

    /// The text with an italic face on each emphasized run and a monospaced face on each code run.
    public static func applying(to text: AttributedString, size: CGFloat) -> AttributedString {
        var styled = text
        for run in text.runs {
            guard let intent = run.inlinePresentationIntent else {
                continue
            }

            if intent.contains(.code) {
                let face = UIFont.monospacedSystemFont(ofSize: size * codeScale, weight: .regular)
                styled[run.range].swiftUI.font = Font(face)
            } else if intent.contains(.emphasized) {
                let weight: UIFont.Weight = intent.contains(.stronglyEmphasized) ? .semibold : .regular
                styled[run.range].swiftUI.font = Font(italicFace(size: size, weight: weight))
            }
        }
        return styled
    }

    static func italicFace(size: CGFloat, weight: UIFont.Weight) -> UIFont {
        let upright = UIFont.systemFont(ofSize: size, weight: weight)
        let traits = upright.fontDescriptor.symbolicTraits.union(.traitItalic)
        guard let descriptor = upright.fontDescriptor.withSymbolicTraits(traits) else {
            return upright
        }
        return UIFont(descriptor: descriptor, size: size)
    }
}
