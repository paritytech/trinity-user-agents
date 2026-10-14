import SwiftUI
import DesignSystem

public struct DisclosureListRow<Leading: View>: View {
    private let title: String
    private let subtitle: String?
    private let leading: Leading
    private let onTap: () -> Void

    public init(
        title: String,
        subtitle: String? = nil,
        @ViewBuilder leading: () -> Leading,
        onTap: @escaping () -> Void
    ) {
        self.title = title
        self.subtitle = subtitle
        self.leading = leading()
        self.onTap = onTap
    }

    public var body: some View {
        Button(action: onTap) {
            HStack(spacing: 12) {
                leading

                VStack(alignment: .leading, spacing: 2) {
                    Text(title)
                        .typography(.titleLarge)
                        .foregroundColor(Color(.fgPrimary))
                        .lineLimit(1)

                    if let subtitle {
                        Text(subtitle)
                            .typography(.bodyMedium)
                            .foregroundColor(Color(.fgSecondary))
                            .lineLimit(1)
                    }
                }

                Spacer()

                Image(.navigationArrowRight)
                    .renderingMode(.template)
                    .foregroundStyle(Color.fgPrimary)
            }
            .padding(.vertical, 4)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

public extension DisclosureListRow where Leading == EmptyView {
    init(title: String, subtitle: String? = nil, onTap: @escaping () -> Void) {
        self.init(title: title, subtitle: subtitle, leading: { EmptyView() }, onTap: onTap)
    }
}
