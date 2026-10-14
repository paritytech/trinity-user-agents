import DesignSystem
import SwiftUI

struct DSChatMarker: View, Hashable {
    enum Style: Hashable {
        case list
        case floated
    }

    let text: String
    let style: Style

    init(text: String, style: Style = .list) {
        self.text = text
        self.style = style
    }

    var body: some View {
        Text(text)
            .typography(.bodyMedium.emphasized)
            .foregroundStyle(Color.fgSecondary)
            .padding(.horizontal, DSSpacings.medium)
            .padding(.vertical, DSSpacings.extraSmall)
            .background(background)
    }
}

private extension DSChatMarker {
    @ViewBuilder
    var background: some View {
        switch style {
        case .list:
            EmptyView()
        case .floated:
            ZStack {
                RoundedRectangle(cornerRadius: DSRadii.extraLarge, style: .continuous)
                    .fill(Color.bgSurfaceContainer)
                RoundedRectangle(cornerRadius: DSRadii.extraLarge, style: .continuous)
                    .strokeBorder(Color.strokeSecondary, lineWidth: 1)
            }
        }
    }
}

#if DEBUG
    #Preview("DSChatMarker variants") {
        VStack(spacing: DSSpacings.medium) {
            DSChatMarker(text: "Today", style: .list)
            DSChatMarker(text: "Today", style: .floated)
            DSChatMarker(text: "Yesterday", style: .floated)
        }
        .padding(DSSpacings.large)
        .background(Color.bgSurfaceMain)
    }
#endif
