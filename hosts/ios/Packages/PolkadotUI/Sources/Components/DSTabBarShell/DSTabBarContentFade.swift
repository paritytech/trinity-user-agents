import Observation
import SwiftUI

/// The opacity of the bar's icons and labels. A quiet hide fades them, and the glass never fades. The host writes it
/// at a fade's endpoints inside `withAnimation`, so SwiftUI interpolates between them. Read it only in leaf views,
/// so a fade re-runs only those views.
@MainActor
@Observable
public final class DSTabBarContentFade {
    /// Opacity from 0 hidden to 1 shown.
    public var opacity: CGFloat = 1

    public init() {}
}
