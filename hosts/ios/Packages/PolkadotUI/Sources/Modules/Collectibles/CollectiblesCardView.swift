import SwiftUI
import DesignSystem

public struct CollectiblesCardView: View {
    let isExpanded: Bool

    public init(isExpanded: Bool = false) {
        self.isExpanded = isExpanded
    }

    public var body: some View {
        Image(.imageCollectibles)
            .saturation(isExpanded ? 1 : 0)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: isExpanded ? .center : .top)
            .clipped()
            .motionShine(.collectibles(isExpanded: isExpanded))
    }
}

#Preview {
    VStack {
        CollectiblesCardView()
        CollectiblesCardView(isExpanded: true)
    }
    .padding()
}
