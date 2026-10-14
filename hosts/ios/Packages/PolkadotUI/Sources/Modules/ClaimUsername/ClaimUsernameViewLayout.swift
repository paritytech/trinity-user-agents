import SwiftUI
import DesignSystem
import ExternalAccessibility
import FoundationExt

@MainActor
public struct ClaimUsernameViewLayout: View {
    private enum Layout {
        static let inputHeight: CGFloat = 56
        static let digitsCardWidth: CGFloat = 72
        static let pickerHeight: CGFloat = 196
        static let separatorDotSize: CGFloat = 4
    }

    @State public private(set) var viewModel: ClaimUsernameViewModel
    @State private var isUsernameFocused: Bool = false
    @State private var showDigits: Bool = false
    @State private var isPickerPresented: Bool = false

    public init(viewModel: ClaimUsernameViewModel) {
        _viewModel = State(initialValue: viewModel)
    }

    public var body: some View {
        ZStack {
            if viewModel.isAccountCreationInProgress {
                accountCreationOverlay
            } else {
                mainContent
            }
        }
        .background(Color.bgSurfaceMain)
        .animation(.easeInOut(duration: 0.25), value: viewModel.isAccountCreationInProgress)
        .overlay {
            pickerOverlay
        }
        .onAppear {
            isUsernameFocused = true
        }
        .onChange(of: viewModel.selectedDigits) { _, _ in
            viewModel.onDigitsChanged?()
        }
    }

    private var mainContent: some View {
        VStack(spacing: 0) {
            VStack(spacing: 24) {
                if !viewModel.headerText.isEmpty {
                    Text(viewModel.headerText)
                        .typography(.titleLarge)
                        .foregroundStyle(Color.fgPrimary)
                        .multilineTextAlignment(.center)
                        .padding(.horizontal, 16)
                        .padding(.top, 11)
                }

                VStack(spacing: 12) {
                    Text(viewModel.title)
                        .typography(.headlineLarge)
                        .foregroundStyle(Color.fgPrimary)
                        .multilineTextAlignment(.center)

                    Text(viewModel.details)
                        .typography(.paragraphLarge)
                        .foregroundStyle(Color.fgTertiary)
                        .multilineTextAlignment(.center)
                }

                inputContainer

                if let availability = viewModel.usernameAvailability {
                    UsernameAvailabilityLabel(viewModel: availability)
                } else {
                    Color.clear
                        .frame(height: 15)
                }
            }
            Spacer()

            actionsView
        }
        .padding(.horizontal, UIConstants.horizontalInsetWide)
        .padding(.bottom, 24)
        .onChange(of: viewModel.digitsState) { _, new in
            if new == .hidden, showDigits {
                withAnimation(.easeInOut(duration: 0.1)) {
                    showDigits = false
                }
            } else if new != .hidden, !showDigits {
                showDigits = true
            }
        }
    }

    private var inputContainer: some View {
        HStack(spacing: 8) {
            if let usernameVM = viewModel.usernameInputViewModel {
                UsernameTextField(
                    inputViewModel: usernameVM,
                    isFocused: $isUsernameFocused,
                    onChange: viewModel.onUsernameChanged
                )
                .disabled(!viewModel.isUsernameInteractionEnabled)
                .frame(height: Layout.inputHeight)
                .cardStyle(background: usernameBorderColor.opacity(0.12), border: usernameBorderColor)
            }
            if showDigits {
                HStack(alignment: .bottom, spacing: 8) {
                    separatorDot
                    digitsSlot
                }
                .transition(.move(edge: .trailing).combined(with: .opacity))
            }
        }
        .frame(height: Layout.inputHeight)
        .animation(.spring(response: 0.45, dampingFraction: 0.72), value: showDigits)
    }

    private var separatorDot: some View {
        Circle()
            .fill(Color.fgTertiary)
            .frame(width: Layout.separatorDotSize, height: Layout.separatorDotSize)
    }

    private var digitsSlot: some View {
        DigitsPeekView(
            selection: viewModel.selectedDigits,
            options: viewModel.digitsOptions
        )
        .blur(radius: viewModel.digitsState == .loading ? 4 : 0)
        .frame(width: Layout.digitsCardWidth, height: Layout.inputHeight)
        .background(digitsBackground)
        .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(digitsBorderColor, lineWidth: 1))
        .opacity(isPickerPresented ? 0 : 1)
        .contentShape(Rectangle())
        .onTapGesture {
            guard viewModel.digitsState != .loading else { return }
            isUsernameFocused = false
            presentPicker()
        }
        .gesture(
            DragGesture(minimumDistance: 10)
                .onEnded { value in
                    guard viewModel.digitsState != .loading,
                          abs(value.translation.height) > 10 else { return }
                    isUsernameFocused = false
                    presentPicker()
                }
        )
    }

//    odor engine frog mother yellow search frown spice seven veteran dance fork
    private var usernameBorderColor: Color {
        switch viewModel.usernameAvailability {
        case .available:
            Color.strokeSuccess
        case .taken,
             .invalid:
            Color.strokeError
        case nil where isUsernameFocused:
            Color.fgPrimary
        case nil:
            Color.strokePrimary
        }
    }

    private var digitsBorderColor: Color {
        Color.strokeSuccess
    }

    // Workaround as we need opaque background
    private var digitsBackground: some View {
        RoundedRectangle(cornerRadius: 12)
            .fill(Color.bgSurfaceMain)
            .overlay(
                RoundedRectangle(cornerRadius: 12)
                    .fill(Color.bgStatusSuccess.opacity(0.12))
            )
    }

    private var actionsView: some View {
        VStack(spacing: 16) {
            if let recoveryString = viewModel.recoveryActionString {
                Button {
                    viewModel.onRecover?()
                } label: {
                    Text(recoveryString)
                }
                .accessibilityId(AccessibilityID.Onboarding.recoverHere)
            }

            ClaimConfirmView(
                state: viewModel.confirmViewState,
                actionTitle: viewModel.actionTitle,
                onAction: viewModel.onConfirm,
                onError: viewModel.onResolveError
            )
            if let terms = viewModel.termsActionString {
                Text(terms)
                    .typography(.paragraphSmall)
                    .foregroundStyle(Color.fgTertiary)
                    .tint(Color.fgPrimary)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 16)
            }
        }
    }

    private var accountCreationOverlay: some View {
        VStack(spacing: 16) {
            ProgressView()
            Text(.creatingAccountDescription)
                .typography(.paragraphLarge)
                .foregroundStyle(Color.fgPrimary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 32)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

// MARK: picker overlay

extension ClaimUsernameViewLayout {
    private var pickerOverlay: some View {
        ZStack {
            if isPickerPresented {
                Color.black.opacity(0.5)
                    .ignoresSafeArea()
                    .contentShape(Rectangle())
                    .onTapGesture { dismissPicker() }
                    .transition(.opacity)
            }
            ghostMainContent
        }
        .allowsHitTesting(isPickerPresented)
    }

    // Mimics main content so digits picker could be shown on the right position
    private var ghostMainContent: some View {
        VStack(spacing: 0) {
            VStack(spacing: 24) {
                if !viewModel.headerText.isEmpty {
                    Text(viewModel.headerText)
                        .typography(.titleLarge)
                        .multilineTextAlignment(.center)
                        .padding(.horizontal, 16)
                        .padding(.top, 11)
                        .opacity(0)
                }
                VStack(spacing: 12) {
                    Text(viewModel.title)
                        .typography(.headlineLarge)
                        .multilineTextAlignment(.center)
                        .opacity(0)
                    Text(viewModel.details)
                        .typography(.paragraphLarge)
                        .multilineTextAlignment(.center)
                        .opacity(0)
                }
                ghostInputContainer
                Color.clear.frame(height: 15)
            }
            Spacer()
        }
        .padding(.horizontal, UIConstants.horizontalInsetWide)
        .padding(.bottom, 24)
    }

    private var ghostInputContainer: some View {
        HStack(spacing: 8) {
            Color.clear
                .frame(maxWidth: .infinity)
                .frame(height: Layout.inputHeight)
            Color.clear
                .frame(width: Layout.separatorDotSize, height: Layout.separatorDotSize)
            Color.clear
                .frame(width: Layout.digitsCardWidth, height: Layout.inputHeight)
                .overlay {
                    if isPickerPresented {
                        DigitsPickerView(
                            selection: $viewModel.selectedDigits,
                            options: viewModel.digitsOptions,
                            onSelectCurrent: { dismissPicker() }
                        )
                        .frame(width: Layout.digitsCardWidth, height: Layout.pickerHeight)
                        .background(digitsBackground)
                        .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(digitsBorderColor, lineWidth: 1))
                        .transition(VerticalExpand(startHeight: Layout.inputHeight, endHeight: Layout.pickerHeight))
                    }
                }
        }
        .frame(height: Layout.inputHeight)
    }

    private func presentPicker() {
        withAnimation(.snappy(duration: 0.25, extraBounce: 0.05)) {
            isPickerPresented = true
        }
    }

    private func dismissPicker() {
        withAnimation(.snappy(duration: 0.25, extraBounce: 0.05)) {
            isPickerPresented = false
        }
    }
}

private extension View {
    func cardStyle(background: Color, border: Color) -> some View {
        let shape = RoundedRectangle(cornerRadius: 12)
        return self
            .background(background, in: shape)
            .overlay(shape.strokeBorder(border, lineWidth: 1))
    }
}

private struct VerticalExpand: Transition {
    let startHeight: CGFloat
    let endHeight: CGFloat

    func body(content: Content, phase: TransitionPhase) -> some View {
        content
            .mask(alignment: .center) {
                Rectangle()
                    .frame(maxWidth: .infinity)
                    .frame(height: phase.isIdentity ? endHeight : startHeight)
            }
            .opacity(phase.isIdentity ? 1 : 0)
    }
}
