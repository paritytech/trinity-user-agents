import DesignSystem
import SwiftUI

// SwiftUI text button built on DSButtonStyle. Mirrors Figma "Button v2":
// leading/trailing icon slots around a centered label.
public struct DSButton: View {
    private let title: String
    private let style: DSButtonStyle.Style
    private let shape: DSButtonStyle.Shape
    private let size: DSButtonStyle.Size
    private let leadingIcon: ImageResource?
    private let trailingIcon: ImageResource?
    private let expands: Bool
    private let action: () -> Void

    public init(
        _ localized: LocalizedStringResource,
        style: DSButtonStyle.Style = .primary,
        shape: DSButtonStyle.Shape = .pill,
        size: DSButtonStyle.Size = .large,
        leadingIcon: ImageResource? = nil,
        trailingIcon: ImageResource? = nil,
        expands: Bool = false,
        action: @escaping () -> Void
    ) {
        title = String(localized: localized)
        self.style = style
        self.shape = shape
        self.size = size
        self.leadingIcon = leadingIcon
        self.trailingIcon = trailingIcon
        self.expands = expands
        self.action = action
    }

    public init(
        _ title: String,
        style: DSButtonStyle.Style = .primary,
        shape: DSButtonStyle.Shape = .pill,
        size: DSButtonStyle.Size = .large,
        leadingIcon: ImageResource? = nil,
        trailingIcon: ImageResource? = nil,
        expands: Bool = false,
        action: @escaping () -> Void
    ) {
        self.title = title
        self.style = style
        self.shape = shape
        self.size = size
        self.leadingIcon = leadingIcon
        self.trailingIcon = trailingIcon
        self.expands = expands
        self.action = action
    }

    public var body: some View {
        Button(action: action) {
            HStack(spacing: DSSpacings.small) {
                if let leadingIcon {
                    icon(leadingIcon)
                }
                Text(title)
                if let trailingIcon {
                    icon(trailingIcon)
                }
            }
            .frame(maxWidth: expands ? .infinity : nil)
        }
        .buttonStyle(.ds(style: style, shape: shape, size: size))
    }

    private func icon(_ resource: ImageResource) -> some View {
        Image(resource)
            .renderingMode(.template)
            .resizable()
            .scaledToFit()
            .frame(width: size.iconSize, height: size.iconSize)
    }
}

#if DEBUG
    #Preview("DSButton") {
        VStack(spacing: DSSpacings.mediumIncreased) {
            DSButton("Continue", expands: true) {}
            DSButton("Secondary", style: .secondary, expands: true) {}
            DSButton("Tertiary", style: .tertiary, expands: true) {}
            DSButton("Destructive", style: .destructive) {}
            DSButton("Ghost", style: .ghost) {}
            DSButton("Large increased", size: .largeIncreased) {}
            DSButton("Rounded medium increased", shape: .rounded, size: .mediumIncreased) {}
            DSButton("Rounded medium", shape: .rounded, size: .medium) {}
            DSButton("Disabled", expands: true) {}
                .disabled(true)
        }
        .padding(DSSpacings.large)
        .background(Color.bgSurfaceMain)
    }
#endif
