import DesignSystem
import Foundation
import ObjectiveC
import UIKit
public import UIKit_iOS

public extension RoundedView {
    func applyBackgroundStyle(_ color: UIColor, cornerRadius: CGFloat) {
        shadowOpacity = 0

        fillColor = color
        highlightedFillColor = color
        self.cornerRadius = cornerRadius
        enableDynamicColorReapply()
    }

    func applyBorderStyle(_ color: UIColor, strokeWidth: CGFloat = 1.0, cornerRadius: CGFloat) {
        shadowOpacity = 0

        fillColor = .clear
        highlightedFillColor = .clear
        strokeColor = color
        self.strokeWidth = strokeWidth
        self.cornerRadius = cornerRadius
        enableDynamicColorReapply()
    }

    func applyBorderStyle(
        _ color: UIColor,
        backgroundColor: UIColor,
        strokeWidth: CGFloat = 1.0,
        cornerRadius: CGFloat
    ) {
        shadowOpacity = 0

        fillColor = backgroundColor
        highlightedFillColor = .clear
        strokeColor = color
        self.strokeWidth = strokeWidth
        self.cornerRadius = cornerRadius
        enableDynamicColorReapply()
    }

    func applyTopCorners(_ cornerRadius: CGFloat) {
        shadowOpacity = 0

        self.cornerRadius = cornerRadius
        roundingCorners = [.topLeft, .topRight]
    }

    /// `RoundedView` paints its fill/stroke as `CALayer` CGColors, which are resolved
    /// once and never re-resolve on a live `DSThemeTrait` switch. Re-apply the stored
    /// dynamic `UIColor`s on theme change so the layer picks up the new theme.
    /// Registers at most once per view instance.
    func enableDynamicColorReapply() {
        guard !isDynamicColorReapplyEnabled else { return }
        isDynamicColorReapplyEnabled = true

        registerForTraitChanges([DSThemeTrait.self]) { (view: RoundedView, _) in
            view.fillColor = view.fillColor
            view.highlightedFillColor = view.highlightedFillColor
            view.strokeColor = view.strokeColor
            view.highlightedStrokeColor = view.highlightedStrokeColor
        }
    }
}

private var dynamicColorReapplyKey: UInt8 = 0

private extension RoundedView {
    var isDynamicColorReapplyEnabled: Bool {
        get { (objc_getAssociatedObject(self, &dynamicColorReapplyKey) as? Bool) ?? false }
        set { objc_setAssociatedObject(self, &dynamicColorReapplyKey, newValue, .OBJC_ASSOCIATION_RETAIN_NONATOMIC) }
    }
}
