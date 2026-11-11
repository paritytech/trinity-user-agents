import SwiftUI
import DesignSystem

public struct DisclosureListRow<Leading: View>: View {
    private let title: String
    private let subtitle: String?
    private let leading: Leading
    private let isEnabled: Bool
    private let onTap: () -> Void

    /// A disabled row dims its title and drops the arrow; `subtitle` can say
    /// why it is closed.
    public init(
        title: String,
        subtitle: String? = nil,
        isEnabled: Bool = true,
        @ViewBuilder leading: () -> Leading,
        onTap: @escaping () -> Void
    ) {
        self.title = title
        self.subtitle = subtitle
        self.leading = leading()
        self.isEnabled = isEnabled
        self.onTap = onTap
    }

    public var body: some View {
        Button(action: onTap) {
            HStack(spacing: 12) {
                leading

                VStack(alignment: .leading, spacing: 2) {
                    Text(title)
                        .typography(.titleLarge)
                        .foregroundColor(isEnabled ? Color(.fgPrimary) : Color(.fgSecondary))
                        .lineLimit(1)

                    if let subtitle {
                        Text(subtitle)
                            .typography(.bodyMedium)
                            .foregroundColor(Color(.fgSecondary))
                            .lineLimit(1)
                    }
                }

                Spacer()

                if isEnabled {
                    Image(.navigationArrowRight)
                        .renderingMode(.template)
                        .foregroundStyle(Color.fgPrimary)
                }
            }
            .padding(.vertical, 4)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!isEnabled)
    }
}

public extension DisclosureListRow where Leading == EmptyView {
    init(title: String, subtitle: String? = nil, onTap: @escaping () -> Void) {
        self.init(title: title, subtitle: subtitle, leading: { EmptyView() }, onTap: onTap)
    }
}
