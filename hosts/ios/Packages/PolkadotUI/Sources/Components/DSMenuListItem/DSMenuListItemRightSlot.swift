import DesignSystem
import SwiftUI

public struct DSMenuListItemRightSlot: View {
    public enum Style {
        case chevron
        case labelChevron(String)
        case labelOnly(String)
        case toggle(Binding<Bool>)
        case radio(isOn: Bool)
    }

    private let style: Style

    public init(_ style: Style) {
        self.style = style
    }

    public var body: some View {
        switch style {
        case .chevron:
            chevronImage
        case let .labelChevron(text):
            HStack(spacing: DSSpacings.small) {
                labelText(text)
                chevronImage
            }
        case let .labelOnly(text):
            labelText(text)
        case let .toggle(isOn):
            Toggle("", isOn: isOn)
                .labelsHidden()
                .tint(Color.fgSuccess)
        case let .radio(isOn):
            DSRadio(isOn: isOn)
        }
    }

    private func labelText(_ text: String) -> some View {
        Text(text)
            .typography(.bodyMedium.emphasized)
            .foregroundStyle(Color.fgSecondary)
            .lineLimit(1)
    }

    private var chevronImage: some View {
        Image(.iconChevronRight)
            .resizable()
            .scaledToFit()
            .frame(width: 20, height: 20)
            .foregroundStyle(Color.fgSecondary)
    }
}

public struct DSMenuListItemIconsSlot<Content: View>: View {
    private let content: () -> Content

    public init(@ViewBuilder content: @escaping () -> Content) {
        self.content = content
    }

    public var body: some View {
        HStack(spacing: DSSpacings.smallIncreased) {
            content()
        }
    }
}

#if DEBUG
    private struct DSMenuListItemRightSlotPreviewWrapper: View {
        @State private var toggleOn = true
        @State private var toggleOff = false

        var body: some View {
            VStack(alignment: .trailing, spacing: 16) {
                DSMenuListItemRightSlot(.labelChevron("Polkadot"))
                DSMenuListItemRightSlot(.labelOnly("USD"))
                DSMenuListItemRightSlot(.toggle($toggleOn))
                DSMenuListItemRightSlot(.toggle($toggleOff))
                DSMenuListItemRightSlot(.radio(isOn: true))
                DSMenuListItemRightSlot(.radio(isOn: false))
            }
            .padding()
            .background(Color.bgSurfaceContainer)
        }
    }

    #Preview("Right slot variants") {
        DSMenuListItemRightSlotPreviewWrapper()
    }
#endif
