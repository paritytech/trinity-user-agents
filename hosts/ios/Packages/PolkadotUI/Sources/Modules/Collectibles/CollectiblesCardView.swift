import SwiftUI
import DesignSystem

public struct CollectiblesCardView: View {
    let isExpanded: Bool
    let onViewCollectibles: () -> Void

    public init(isExpanded: Bool = false, onViewCollectibles: @escaping () -> Void = {}) {
        self.isExpanded = isExpanded
        self.onViewCollectibles = onViewCollectibles
    }

    public var body: some View {
        ZStack {
            Image(.imageCollectibles)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: isExpanded ? .center : .top)
                .clipped()
                .motionShine(.collectibles(isExpanded: isExpanded))

            if isExpanded {
                DSButton(String(localized: .collectiblesCardActionView), action: onViewCollectibles)
                    .transition(.asymmetric(insertion: .opacity, removal: .identity))
            }
        }
    }
}

#Preview {
    VStack {
        CollectiblesCardView()
        CollectiblesCardView(isExpanded: true)
    }
    .padding()
}
