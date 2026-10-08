import DesignSystem
import PolkadotUI
import SwiftUI

/// The title row every funding screen opens with: a round back button, the
/// title centred over the sheet, and room for one trailing control.
struct FundingScreenHeader<Trailing: View>: View {
    let title: String
    var leadingTitle = false
    var onBack: (() -> Void)?
    @ViewBuilder var trailing: () -> Trailing

    var body: some View {
        ZStack {
            Text(title)
                .typography(leadingTitle ? .titleSmall : .titleMedium)
                .foregroundStyle(.fgPrimary)
                .frame(maxWidth: .infinity, alignment: leadingTitle ? .leading : .center)
                .padding(.horizontal, onBack == nil ? 0 : 48)

            HStack {
                if let onBack {
                    FundingCircleButton(systemImage: "chevron.left", action: onBack)
                }
                Spacer()
                trailing()
            }
        }
        .frame(height: 44)
    }
}

extension FundingScreenHeader where Trailing == EmptyView {
    init(title: String, leadingTitle: Bool = false, onBack: (() -> Void)?) {
        self.title = title
        self.leadingTitle = leadingTitle
        self.onBack = onBack
        trailing = { EmptyView() }
    }
}

struct FundingCircleButton: View {
    let systemImage: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: systemImage)
                .font(.system(size: 16, weight: .semibold))
                .foregroundStyle(.fgPrimary)
                .frame(width: 40, height: 40)
                .background(.bgSurfaceMain, in: Circle())
        }
    }
}

/// One label and value row of a summary.
struct FundingValueRow<Value: View>: View {
    let title: String
    var action: (() -> Void)?
    @ViewBuilder var value: () -> Value

    var body: some View {
        Button {
            action?()
        } label: {
            HStack(alignment: .firstTextBaseline, spacing: DSSpacings.small) {
                Text(title)
                    .typography(.bodyLarge)
                    .foregroundStyle(.fgSecondary)
                Spacer(minLength: DSSpacings.small)
                value()
                if action != nil {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(.fgPrimary)
                }
            }
            .padding(.vertical, DSSpacings.small)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(action == nil)
    }
}

/// A round letter mark in a colour, for networks, tokens and providers that
/// have no image of their own.
struct FundingMonogram: View {
    let text: String
    let tint: Color
    var size: CGFloat = 32

    var body: some View {
        Text(text)
            .font(.system(size: size * 0.45, weight: .bold))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(tint, in: Circle())
    }
}

struct FundingProviderLogo: View {
    let brand: FundingProviderBrand
    var size: CGFloat = 32

    var body: some View {
        if let icon = brand.icon {
            Image(uiImage: icon)
                .resizable()
                .scaledToFill()
                .frame(width: size, height: size)
                .clipShape(Circle())
        } else {
            FundingMonogram(text: String(brand.name.prefix(1)).uppercased(), tint: .gray, size: size)
        }
    }
}

/// A grey bar standing in for text that is still loading.
struct FundingSkeleton: View {
    var width: CGFloat = 80
    var height: CGFloat = 14

    var body: some View {
        RoundedRectangle(cornerRadius: height / 2)
            .fill(Color.bgSurfaceNested)
            .frame(width: width, height: height)
    }
}

/// A short confirmation pill along the bottom of a screen.
struct FundingToast: View {
    let message: String

    var body: some View {
        HStack(spacing: DSSpacings.small) {
            Image(systemName: "checkmark")
                .font(.system(size: 13, weight: .semibold))
            Text(message)
                .typography(.bodySmall)
        }
        .foregroundStyle(.fgPrimary)
        .padding(.horizontal, DSSpacings.medium)
        .padding(.vertical, DSSpacings.small)
        .background(.bgSurfaceNested, in: Capsule())
    }
}

extension View {
    /// Shows `message` for two seconds, then clears it.
    func fundingToast(_ message: Binding<String?>) -> some View {
        overlay(alignment: .bottom) {
            if let text = message.wrappedValue {
                FundingToast(message: text)
                    .padding(.bottom, DSSpacings.large)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
                    .task(id: text) {
                        try? await Task.sleep(for: .seconds(2))
                        withAnimation { message.wrappedValue = nil }
                    }
            }
        }
        .animation(.easeInOut, value: message.wrappedValue)
    }
}

/// The primary action pinned to the bottom of a funding screen.
struct FundingPrimaryButton: View {
    let title: String
    var style: DSButtonStyle.Style = .primary
    var isEnabled = true
    var isLoading = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Group {
                if isLoading {
                    ProgressView().tint(.fgPrimaryInverted)
                } else {
                    Text(title)
                }
            }
            .frame(maxWidth: .infinity)
        }
        .buttonStyle(.ds(style: style, shape: .pill, size: .large))
        .disabled(!isEnabled || isLoading)
    }
}
