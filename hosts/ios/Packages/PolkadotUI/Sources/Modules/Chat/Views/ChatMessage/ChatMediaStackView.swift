import DesignSystem
import SwiftUI
import UIKit

/// Gives the picture for one media source of a chat body. The host decides how a source resolves.
public struct ChatImageLoader: Sendable {
    private let load: @Sendable (String) async -> UIImage?

    public init(_ load: @escaping @Sendable (String) async -> UIImage?) {
        self.load = load
    }

    public func callAsFunction(_ source: String) async -> UIImage? {
        await load(source)
    }
}

/// Media of a product message, one image below the other, with optional text below them.
public struct ChatMediaStackView: View, Hashable {
    public static let reuseIdentifier = "ChatMediaStackView"

    private enum Layout {
        static let imageHeight: CGFloat = 200
    }

    let sources: [String]
    /// The text alternative of each source, for VoiceOver (P7.2).
    public let alts: [String?]
    let text: ChatMarkdownText?
    let loader: ChatImageLoader

    public init(sources: [String], alts: [String?] = [], text: ChatMarkdownText?, loader: ChatImageLoader) {
        self.sources = sources
        self.alts = alts
        self.text = text
        self.loader = loader
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: DSSpacings.extraTiny) {
            ForEach(Array(sources.enumerated()), id: \.offset) { index, source in
                ChatRemoteImage(source: source, loader: loader)
                    .frame(maxWidth: .infinity)
                    .frame(height: Layout.imageHeight)
                    .clipped()
                    .accessibilityElement(children: .ignore)
                    .accessibilityAddTraits(.isImage)
                    .accessibilityLabel(Text(alt(at: index) ?? ""))
                    .accessibilityHidden(alt(at: index) == nil)
            }

            if let text {
                text
                    .padding(.horizontal, DSSpacings.small + DSSpacings.tiny)
                    .padding(.vertical, DSSpacings.extraSmall)
            }
        }
    }

    public static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.sources == rhs.sources && lhs.alts == rhs.alts && lhs.text == rhs.text
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(sources)
        hasher.combine(alts)
        hasher.combine(text)
    }

    /// The text alternative of the source at `index`, or nil when the product gave none.
    public func alt(at index: Int) -> String? {
        guard alts.indices.contains(index), let alt = alts[index],
              !alt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return nil
        }
        return alt
    }
}

private struct ChatRemoteImage: View {
    let source: String
    let loader: ChatImageLoader

    @State private var image: UIImage?
    @State private var failed = false

    var body: some View {
        ZStack {
            Color(.bgSurfaceContainer)

            if let image {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFill()
                    .transition(.opacity)
            } else if failed {
                Image(systemName: "photo")
                    .foregroundStyle(.fgTertiary)
            } else {
                ProgressView()
            }
        }
        .animation(.easeInOut, value: image)
        .task(id: source) {
            let loaded = await loader(source)
            image = loaded
            failed = loaded == nil
        }
    }
}
