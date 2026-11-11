import SwiftUI
import DesignSystem

public struct CollectiblesCardView: View {
    let isColoured: Bool

    public init(isColoured: Bool = false) {
        self.isColoured = isColoured
    }

    public var body: some View {
        Image(.imageCollectibles)
            .saturation(isColoured ? 1 : 0)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .clipped()
            .motionShine(.collectibles(isExpanded: false))
    }
}

#Preview {
    VStack {
        CollectiblesCardView()
        CollectiblesCardView(isColoured: true)
    }
    .padding()
}
