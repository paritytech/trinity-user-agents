import SwiftUI
import DesignSystem

public struct AppIconPlaceholder: View {
    public let size: CGFloat
    public let cornerRadius: CGFloat

    public init(size: CGFloat, cornerRadius: CGFloat) {
        self.size = size
        self.cornerRadius = cornerRadius
    }

    public var body: some View {
        RoundedRectangle(cornerRadius: cornerRadius)
            .fill(Color(.fgSecondary))
            .frame(width: size, height: size)
    }
}
