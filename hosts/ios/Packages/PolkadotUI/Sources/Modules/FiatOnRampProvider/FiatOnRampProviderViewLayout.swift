import Foundation
import SwiftUI

public struct FiatOnRampProviderViewLayout: View {
    @Bindable var viewModel: FiatOnRampProviderViewModel

    public init(viewModel: FiatOnRampProviderViewModel) {
        self.viewModel = viewModel
    }

    public var body: some View {
        ZStack {
            VStack(spacing: 0) {
                Text(.FiatOnRamp.fiatOnrampProvidersTitle)
                    .textStyle(.title32SemiBold())
                    .foregroundStyle(.white)
                    .frame(maxWidth: .infinity, alignment: .center)
                    .padding(.top, 16)
                    .padding(.bottom, 40)

                List {
                    Section {
                        if viewModel.isLoading {
                            ForEach(0 ..< 6, id: \.self) { _ in
                                SkeletonRow()
                                    .listRowSeparator(.hidden)
                                    .listRowBackground(Color.clear)
                                    .listRowInsets(
                                        EdgeInsets(top: 0, leading: 16, bottom: 0, trailing: 16)
                                    )
                            }
                        }

                        if viewModel.viewModels.isEmpty, !viewModel.isLoading {
                            Text(.FiatOnRamp.fiatOnrampProvidersEmpty)
                                .textStyle(.body14Regular())
                                .foregroundStyle(Color(.textAndIconsPrimaryDark))
                                .multilineTextAlignment(.center)
                                .padding(.top, 24)
                                .listRowSeparator(.hidden)
                                .listRowBackground(Color.clear)
                                .listRowInsets(
                                    EdgeInsets(top: 0, leading: 16, bottom: 0, trailing: 16)
                                )
                        }

                        ForEach(viewModel.viewModels) { model in
                            Button {
                                viewModel.onSelect?(model)
                            } label: {
                                ProviderRow(model: model)
                            }
                            .disabled(viewModel.isLoading)
                            .listRowSeparator(.hidden)
                            .listRowBackground(Color.clear)
                            .listRowInsets(
                                EdgeInsets(top: 0, leading: 16, bottom: 0, trailing: 16)
                            )
                        }
                    } footer: {
                        if !viewModel.isLoading {
                            footer
                                .listRowInsets(
                                    EdgeInsets(top: 8, leading: 16, bottom: 8, trailing: 16)
                                )
                                .listRowSeparator(.hidden)
                                .listRowBackground(Color.clear)
                        }
                    }
                }
                .listStyle(.plain)
                .listRowSpacing(8)
                .scrollContentBackground(.hidden)
            }
            .sheet(
                item: confirmationBinding,
                onDismiss: { viewModel.confirmation = nil }
            ) { confirmation in
                let host = confirmation.url.host ?? confirmation.url.absoluteString
                let message = String(localized: .FiatOnRamp.fiatOnrampBrowserAlertMessage(host))

                FiatOnRampProviderBrowserConfirmView(
                    title: String(localized: .FiatOnRamp.fiatOnrampBrowserAlertTitle),
                    message: message,
                    onCancel: { viewModel.confirmation = nil },
                    onConfirm: {
                        let url = confirmation.url
                        viewModel.confirmation = nil
                        // Add delay before presenting to allow the confirmation sheet to be dismissed first.
                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                            viewModel.onConfirmOpenUrl?(url)
                        }
                    }
                )
                .presentationDetents([.fraction(0.25)])
                .presentationDragIndicator(.hidden)
                .presentationBackground(Color.clear)
            }

            if viewModel.isWidgetLoading {
                Color(.backgroundPrimary)
                    .opacity(0.6)
                    .ignoresSafeArea()

                ProgressView()
                    .progressViewStyle(.circular)
                    .tint(Color(.textAndIconsPrimaryDark))
            }
        }
        .background(Color(.backgroundPrimary))
    }

    private var footer: some View {
        HStack(spacing: 6) {
            if viewModel.isRefreshing {
                Text(.FiatOnRamp.fiatOnrampProvidersRefreshing)
                    .textStyle(.title16Medium())
                    .foregroundStyle(Color(.textAndIconsSecondary))
                    .lineLimit(1)

                ProgressView()
                    .progressViewStyle(.circular)
                    .tint(Color(.textAndIconsSecondary))
                    .scaleEffect(0.75)
            } else if let refreshText = viewModel.refreshCountdownText {
                Text(refreshText)
                    .textStyle(.title16Medium())
                    .foregroundStyle(Color(.textAndIconsSecondary))
                    .lineLimit(1)
            }
        }
        .frame(maxWidth: .infinity)
        .opacity((viewModel.isRefreshing || viewModel.refreshCountdownText != nil) ? 1 : 0)
    }

    private var confirmationBinding: Binding<FiatOnRampProviderConfirmation?> {
        Binding(
            get: { viewModel.confirmation },
            set: { viewModel.confirmation = $0 }
        )
    }
}

private extension FiatOnRampProviderViewLayout {
    struct ProviderRow: View {
        let model: FiatOnRampProviderItemViewModel

        var body: some View {
            HStack(spacing: 16) {
                Group {
                    if let icon = model.icon {
                        AsyncImageView(
                            viewModel: icon,
                            settings: ImageViewModelSettings(
                                targetSize: CGSize(width: 32, height: 32)
                            )
                        )
                        .frame(width: 32, height: 32)
                    } else {
                        Color.clear
                            .frame(width: 32, height: 32)
                    }
                }
                .frame(width: 48, height: 48)

                Text(model.name)
                    .textStyle(.title18SemiBold())
                    .foregroundStyle(.white)
                    .lineLimit(1)

                Spacer()

                if let quoteText = model.quoteText {
                    VStack(alignment: .trailing, spacing: 2) {
                        if let fiatText = model.fiatAmountText {
                            Text(fiatText)
                                .textStyle(.title16Medium())
                                .foregroundStyle(.white)
                                .lineLimit(1)
                        }

                        Text(quoteText)
                            .textStyle(.body14Regular())
                            .foregroundStyle(.white.opacity(0.48))
                            .lineLimit(1)
                    }
                    .multilineTextAlignment(.trailing)
                }
            }
            .foregroundStyle(Color(.textAndIconsPrimaryDark))
            .padding(.horizontal, 16)
            .padding(.vertical, 16)
            .frame(maxWidth: .infinity, minHeight: 80)
            .background(Color(.fill12), in: RoundedRectangle(cornerRadius: 24))
        }
    }

    struct SkeletonRow: View {
        var body: some View {
            HStack(spacing: 12) {
                Circle()
                    .fill(Color(.fill12))
                    .shimmering()
                    .frame(width: 48, height: 48)

                RoundedRectangle(cornerRadius: 6)
                    .fill(Color(.fill12))
                    .shimmering()
                    .frame(width: 120, height: 12)

                Spacer()

                VStack(alignment: .trailing, spacing: 6) {
                    RoundedRectangle(cornerRadius: 6)
                        .fill(Color(.fill12))
                        .shimmering()
                        .frame(width: 52, height: 10)

                    RoundedRectangle(cornerRadius: 6)
                        .fill(Color(.fill12))
                        .shimmering()
                        .frame(width: 36, height: 10)
                }
            }
            .frame(height: 72)
            .padding(.horizontal, 16)
            .background(Color(.fill12), in: RoundedRectangle(cornerRadius: 16))
        }
    }
}

private extension View {
    func shimmering() -> some View {
        modifier(ShimmerModifier())
    }
}

private struct ShimmerModifier: ViewModifier {
    @State private var offset: CGFloat = -1

    func body(content: Content) -> some View {
        content
            .overlay(
                GeometryReader { proxy in
                    let width = proxy.size.width

                    LinearGradient(
                        colors: [
                            Color.white.opacity(0.0),
                            Color.white.opacity(0.35),
                            Color.white.opacity(0.0)
                        ],
                        startPoint: .top,
                        endPoint: .bottom
                    )
                    .frame(width: width * 0.6)
                    .offset(x: width * offset)
                }
                .clipped()
                .allowsHitTesting(false)
            )
            .mask(content)
            .onAppear {
                withAnimation(.linear(duration: 1.2).repeatForever(autoreverses: false)) {
                    offset = 1.2
                }
            }
    }
}

#Preview("Providers Loading") {
    let loadingViewModel = FiatOnRampProviderViewModel()
    loadingViewModel.isLoading = true

    return FiatOnRampProviderViewLayout(viewModel: loadingViewModel)
}

#Preview("Providers Ready") {
    let readyViewModel = FiatOnRampProviderViewModel()
    readyViewModel.viewModels = [
        .init(
            id: "provider-1",
            name: "Provider One",
            icon: nil,
            quoteText: "0.123456 DOT",
            fiatAmountText: "$120.00"
        ),
        .init(
            id: "provider-2",
            name: "Provider Two",
            icon: nil,
            quoteText: "0.098765 DOT",
            fiatAmountText: "$98.76"
        )
    ]
    readyViewModel.refreshCountdownText = "New quote: 0:29"

    return FiatOnRampProviderViewLayout(viewModel: readyViewModel)
}
