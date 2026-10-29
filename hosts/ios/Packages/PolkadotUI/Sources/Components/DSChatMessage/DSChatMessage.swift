import DesignSystem
import SwiftUI

public struct DSChatMessage: View, Equatable {
    public enum Kind: Hashable {
        case `default`
        case media
        case voiceCall
        case videoCall
    }

    public enum Context: Hashable {
        case single
        case group(senderName: String)
    }

    private let text: String
    private let kind: Kind
    private let context: Context
    /// The text tells of an activity that continues, for example "Ben is typing", so the row animates it.
    private let isOngoing: Bool

    public init(text: String, kind: Kind = .default, context: Context = .single, isOngoing: Bool = false) {
        self.text = text
        self.kind = kind
        self.context = context
        self.isOngoing = isOngoing
    }

    public var body: some View {
        switch context {
        case .single:
            messageRow(lineLimit: 2)
        case let .group(senderName):
            VStack(alignment: .leading, spacing: 0) {
                Text(senderName)
                    .typography(.paragraphMedium)
                    .foregroundStyle(Color.fgPrimary)
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
                messageRow(lineLimit: 1)
            }
        }
    }

    private func messageRow(lineLimit: Int) -> some View {
        HStack(alignment: kind.rowAlignment, spacing: DSSpacings.extraSmall) {
            if isOngoing {
                DSTypingDots()
                    .frame(height: DSChatMessageMetrics.iconSize)
            }
            if let resource = kind.iconResource {
                kindIcon(resource)
            }
            Text(text)
                .typography(.paragraphMedium)
                .foregroundStyle(Color.fgSecondary)
                .lineLimit(lineLimit)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func kindIcon(_ resource: ImageResource) -> some View {
        Image(resource)
            .foregroundStyle(Color.fgSecondary)
            .frame(width: DSChatMessageMetrics.iconSize, height: DSChatMessageMetrics.iconSize)
    }
}

private enum DSChatMessageMetrics {
    static let iconSize: CGFloat = 16
}

/// Three dots that become bright one after the other. They show an activity that continues.
struct DSTypingDots: View {
    private static let dotCount = 3
    private static let step: TimeInterval = 0.4
    private static let dotSize: CGFloat = 4

    var body: some View {
        TimelineView(.periodic(from: .now, by: Self.step)) { timeline in
            let litDot = Int(timeline.date.timeIntervalSinceReferenceDate / Self.step) % Self.dotCount

            HStack(spacing: Self.dotSize / 2) {
                ForEach(0 ..< Self.dotCount, id: \.self) { dot in
                    Circle()
                        .fill(Color.fgSecondary)
                        .frame(width: Self.dotSize, height: Self.dotSize)
                        .opacity(dot == litDot ? 1 : 0.3)
                }
            }
            .animation(.easeInOut(duration: Self.step), value: litDot)
        }
    }
}

private extension DSChatMessage.Kind {
    // Matches Figma: media uses items-start (icon top-aligned for wrapping text);
    // voice/video are single-line so icon centers with the line.
    var rowAlignment: VerticalAlignment {
        switch self {
        case .default,
             .media: .top
        case .voiceCall,
             .videoCall: .center
        }
    }

    var iconResource: ImageResource? {
        switch self {
        case .default: nil
        case .media: .icon16PhotoSolid
        case .voiceCall: .icon16PhoneCall
        case .videoCall: .icon16Video
        }
    }
}

#if DEBUG
    #Preview("Messages") {
        VStack(alignment: .leading, spacing: 16) {
            DSChatMessage(text: "Hey there! How can I assist you today?")
            DSChatMessage(text: "Media content with 2 lines of looooooong text wrapping nicely", kind: .media)
            DSChatMessage(text: "Voice call", kind: .voiceCall)
            DSChatMessage(text: "Video call", kind: .videoCall)
            DSChatMessage(text: "Ben is typing", isOngoing: true)
            Divider().background(Color.strokePrimary)
            DSChatMessage(
                text: "Hey there! How can I assist you today?",
                context: .group(senderName: "person.99")
            )
            DSChatMessage(
                text: "Media content",
                kind: .media,
                context: .group(senderName: "person.99")
            )
            DSChatMessage(
                text: "Voice call",
                kind: .voiceCall,
                context: .group(senderName: "person.99")
            )
        }
        .padding(24)
        .background(Color.bgSurfaceMain)
    }
#endif
