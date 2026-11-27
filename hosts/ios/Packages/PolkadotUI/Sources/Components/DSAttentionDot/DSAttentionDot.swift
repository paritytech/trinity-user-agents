import DesignSystem
import SwiftUI

public struct DSAttentionDot: View {
    public static let size = DSSpacings.extraSmall

    public init() {}

    public var body: some View {
        Circle()
            .fill(Color.fgWarning)
            .frame(width: Self.size, height: Self.size)
    }
}

public extension View {
    func attentionDot(_ isVisible: Bool) -> some View {
        overlay(alignment: .topTrailing) {
            if isVisible {
                DSAttentionDot()
                    .transition(.opacity)
            }
        }
    }
}
