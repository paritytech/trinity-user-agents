import SwiftUI
import ExternalAccessibility
import PolkadotUI
import WebRTC

extension ChatCallViewLayout {
    struct ViewModel {
        let username: String
        let avatarViewModel: AvatarViewModel
        let callType: ChatCallType
        let isIncoming: Bool
    }
}

struct ChatCallViewLayout: View {
    @State var viewModel = ChatCallViewModel()

    var body: some View {
        GeometryReader { proxy in
            ZStack {
                // Background
                Color.bgSurfaceMain
                    .ignoresSafeArea()

                // Remote video (if available and call is connected)
                if let remoteModel = viewModel.remoteRenderingModel,
                   remoteModel.hasVideo {
                    RTCVideoViewRepresentable(model: remoteModel)
                        .frame(width: proxy.size.width, height: proxy.size.height)
                        .ignoresSafeArea() // extend under safe areas
                        .aspectRatio(contentMode: .fill) // SwiftUI side fill
                        .clipped()
                        .opacity(viewModel.shouldShowRemoteVideo ? 1 : 0)
                }

                if let localModel = viewModel.localRenderingModel, localModel.hasVideo {
                    let pipWidth: CGFloat = 120
                    let pipHeight: CGFloat = 180
                    let topPadding: CGFloat = 64
                    let trailingPadding: CGFloat = 12

                    RTCVideoViewRepresentable(model: localModel)
                        .frame(width: pipWidth, height: pipHeight)
                        .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
                        .overlay(
                            RoundedRectangle(cornerRadius: 16)
                                .stroke(.white.opacity(0.4), lineWidth: 1)
                        )
                        .shadow(radius: 6)
                        .opacity(viewModel.isVideoEnabled ? 1 : 0)
                        // Center-based positioning in the top-right corner
                        .position(
                            x: proxy.size.width - pipWidth / 2 - trailingPadding,
                            y: proxy.safeAreaInsets.top + pipHeight / 2 + topPadding
                        )
                }

                if !viewModel.shouldShowRemoteVideo {
                    // Overlay content
                    VStack(spacing: 0) {
                        // Avatar with animated rings
                        AvatarWithRingsView(
                            avatarViewModel: viewModel.avatarViewModel,
                            isRinging: viewModel.callState.isRingingState
                        )

                        // Username
                        Text(viewModel.username)
                            .typography(.headlineLarge)
                            .foregroundStyle(Color.fgPrimary)
                            .padding(.top, 32)

                        // Call status or duration
                        callStatusOrDuration
                            .padding(.top, 8)
                    }
                }
            } // 🔹 GeometryReader overlay ONLY for the local PiP
        }
        .overlay {
            VStack {
                Spacer()

                micOffBanners
                    .padding(.bottom, 24)

                // Call action buttons
                HStack(spacing: 24) {
                    // Accept button (only for incoming calls in ringing state)
                    if viewModel.isIncoming, viewModel.callState == .ringing {
                        Button {
                            viewModel.onAcceptCall?()
                        } label: {
                            Image(systemName: "phone.fill")
                                .font(.system(size: 24, weight: .semibold))
                                .foregroundStyle(Color.fgStaticWhite)
                                .frame(width: 64, height: 64)
                                .background(Color.bgStatusSuccess, in: Circle())
                        }
                        .accessibilityId(AccessibilityID.IncomingCall.answerButton)
                    }

                    if viewModel.shouldDisplayAudioRoute {
                        AudioRouteMenuButton(
                            state: viewModel.audioRouteState,
                            onSelect: { route in
                                viewModel.onSelectAudioRoute?(route)
                            }
                        )
                        .accessibilityId(AccessibilityID.InCall.volumeButton)
                    }

                    if viewModel.shouldDisplayMute {
                        Button {
                            viewModel.onToggleMute?()
                        } label: {
                            Image(systemName: viewModel.isMuted ? "mic.slash.fill" : "mic.fill")
                                .font(.system(size: 24, weight: .semibold))
                                .foregroundStyle(Color.fgPrimary)
                                .frame(width: 64, height: 64)
                                .background(
                                    Color.bgActionTertiary,
                                    in: Circle()
                                )
                        }
                        .accessibilityId(AccessibilityID.InCall.muteButton)
                    }

                    if viewModel.shouldDisplayVideoToggle {
                        Button {
                            viewModel.onToggleVideo?()
                        } label: {
                            Image(systemName: viewModel.isVideoEnabled ? "video.fill" : "video.slash.fill")
                                .font(.system(size: 24, weight: .semibold))
                                .foregroundStyle(Color.fgPrimary)
                                .frame(width: 64, height: 64)
                                .background(
                                    Color.bgActionTertiary,
                                    in: Circle()
                                )
                        }
                    }

                    if viewModel.canEndCall {
                        Button {
                            viewModel.onEndCall?()
                        } label: {
                            Image(systemName: "phone.down.fill")
                                .font(.system(size: 24, weight: .semibold))
                                .foregroundStyle(Color.fgStaticWhite)
                                .frame(width: 64, height: 64)
                                .background(Color.bgStatusError, in: Circle())
                        }
                        .accessibilityId(viewModel.endCallAccessibilityId)
                    }
                }
                .padding(.bottom, 40)
            }
        }
        .ignoresSafeArea()
        .onAppear {
            // Start ringing animation when view appears
            if viewModel.callState == .ringing {
                // Animation is handled by AvatarWithRingsView
            }
        }
    }

    @ViewBuilder
    private var callStatusOrDuration: some View {
        if viewModel.callState.showsDuration, let connectedAt = viewModel.connectedAt {
            TimelineView(.periodic(from: connectedAt, by: 1)) { context in
                Text(ChatCallDurationFormatter.string(from: context.date.timeIntervalSince(connectedAt)))
                    .typography(.bodyLarge)
                    .foregroundStyle(Color(.fgTertiary))
                    .monospacedDigit()
            }
            .accessibilityId(AccessibilityID.InCall.statusLabel)
        } else if let text = viewModel.callState.statusText {
            Text(text)
                .typography(.bodyLarge)
                .foregroundStyle(Color(.fgTertiary))
                .accessibilityId(viewModel.statusAccessibilityId)
        }
    }

    private var micOffBanners: some View {
        VStack(spacing: 8) {
            if viewModel.shouldShowSelfMicOffBanner {
                MicOffBanner(text: String(localized: .chatCallSelfMicOff))
            }

            if viewModel.shouldShowRemoteMicOffBanner {
                MicOffBanner(text: String(localized: .chatCallRemoteMicOff(viewModel.username)))
            }
        }
    }
}

// MARK: - Avatar with Animated Rings

private struct AvatarWithRingsView: View {
    let avatarViewModel: AvatarViewModel
    let isRinging: Bool

    @State private var ring1Scale: CGFloat = 1.0
    @State private var ring1Opacity: Double = 1.0
    @State private var ring2Scale: CGFloat = 1.0
    @State private var ring2Opacity: Double = 1.0
    @State private var ring3Scale: CGFloat = 1.0
    @State private var ring3Opacity: Double = 1.0

    var body: some View {
        ZStack {
            // Ring 3 (outermost, starts last)
            if isRinging {
                RingView()
                    .scaleEffect(ring3Scale)
                    .opacity(ring3Opacity)
            }

            // Ring 2 (middle)
            if isRinging {
                RingView()
                    .scaleEffect(ring2Scale)
                    .opacity(ring2Opacity)
            }

            // Ring 1 (innermost, starts first)
            if isRinging {
                RingView()
                    .scaleEffect(ring1Scale)
                    .opacity(ring1Opacity)
            }

            // Avatar
            DSAvatar(viewModel: avatarViewModel, size: .s136)
        }
        .frame(width: 240, height: 240)
        .onAppear {
            if isRinging {
                startAnimations()
            }
        }
        .onChange(of: isRinging) { _, newValue in
            if newValue {
                startAnimations()
            } else {
                stopAnimations()
            }
        }
    }

    private func startAnimations() {
        // Ring 1 animation
        withAnimation(
            .easeOut(duration: 2.0)
                .repeatForever(autoreverses: false)
        ) {
            ring1Scale = 1.8
            ring1Opacity = 0.0
        }

        // Ring 2 animation (delayed)
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) {
            withAnimation(
                .easeOut(duration: 2.0)
                    .repeatForever(autoreverses: false)
            ) {
                ring2Scale = 1.8
                ring2Opacity = 0.0
            }
        }

        // Ring 3 animation (delayed more)
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) {
            withAnimation(
                .easeOut(duration: 2.0)
                    .repeatForever(autoreverses: false)
            ) {
                ring3Scale = 1.8
                ring3Opacity = 0.0
            }
        }
    }

    private func stopAnimations() {
        withAnimation {
            ring1Scale = 1.0
            ring1Opacity = 1.0
            ring2Scale = 1.0
            ring2Opacity = 1.0
            ring3Scale = 1.0
            ring3Opacity = 1.0
        }
    }
}

// MARK: - Ring View

private struct RingView: View {
    var body: some View {
        Circle()
            .stroke(
                Color.fgPrimary.opacity(0.3),
                lineWidth: 2
            )
            .frame(
                width: DSLetterAvatar.Size.s136.dimension,
                height: DSLetterAvatar.Size.s136.dimension
            )
    }
}

// MARK: - Mic Off Banner

private struct MicOffBanner: View {
    let text: String

    var body: some View {
        Label(text, systemImage: "mic.slash.fill")
            .typography(.bodyLarge)
            .foregroundStyle(Color.fgPrimary)
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            .background(Color.bgActionTertiary, in: Capsule())
    }
}

// MARK: - Audio Route Menu Button

private struct AudioRouteMenuButton: View {
    let state: CallAudioRouteState
    let onSelect: (CallAudioRoute) -> Void

    var body: some View {
        Menu {
            ForEach(state.availableRoutes, id: \.self) { route in
                Toggle(
                    isOn: Binding<Bool>(
                        get: { state.selectedRoute == route },
                        set: { isOn in
                            if isOn { onSelect(route) }
                        }
                    )
                ) {
                    Label(route.displayName, systemImage: route.iconName)
                }
            }
        } label: {
            buttonLabel
        }
        .menuOrder(.fixed)
    }

    private var isActive: Bool {
        state.isUsingNonReceiver
    }

    private var buttonLabel: some View {
        ZStack {
            Circle()
                .fill(isActive ? Color.bgActionPrimary : Color.bgActionTertiary)

            Image(systemName: state.outputIconName)
                .font(.system(size: 24, weight: .semibold))
                .foregroundStyle(
                    isActive ? Color.fgPrimaryInverted : Color.fgPrimary
                )
                .contentTransition(.symbolEffect(.replace))
        }
        .frame(width: 64, height: 64)
        .animation(.smooth(duration: 0.25), value: state.outputIconName)
        .animation(.smooth(duration: 0.25), value: isActive)
    }
}

// MARK: - RTCVideoView Representable

private struct RTCVideoViewRepresentable: UIViewRepresentable {
    let model: ChatCallRendererModel

    func makeUIView(context _: Context) -> UIView {
        let view = WebRTCContainerView()

        model.attach?(view.rtcView)

        return view
    }

    func updateUIView(_: UIView, context _: Context) {}
}

final class WebRTCContainerView: UIView {
    let rtcView = RTCMTLVideoView()

    override init(frame: CGRect) {
        super.init(frame: frame)
        rtcView.translatesAutoresizingMaskIntoConstraints = false
        rtcView.videoContentMode = .scaleAspectFill
        rtcView.layer.masksToBounds = true

        addSubview(rtcView)
        NSLayoutConstraint.activate([
            rtcView.leadingAnchor.constraint(equalTo: leadingAnchor),
            rtcView.trailingAnchor.constraint(equalTo: trailingAnchor),
            rtcView.topAnchor.constraint(equalTo: topAnchor),
            rtcView.bottomAnchor.constraint(equalTo: bottomAnchor)
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}
