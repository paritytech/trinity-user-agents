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

/// One media item of a product message (02-DESIGN §6.2).
public struct ChatMediaStackItem: Hashable {
    public enum Kind: Hashable {
        case image
        /// A video. The bubble shows its poster, and a tap plays it.
        case video
        /// A file. The bubble shows a row with its name and `detail`, for example its size.
        case file(name: String, detail: String)
    }

    public let source: String
    public let kind: Kind
    /// The text alternative of the item, for VoiceOver (P7.2).
    public let alt: String?

    public init(source: String, kind: Kind, alt: String?) {
        self.source = source
        self.kind = kind
        self.alt = alt
    }
}

/// What a tap on a media item does. The host opens an image full screen, plays a video and lets the user open or
/// share a file.
public struct ChatMediaOpening {
    public struct Titles {
        let retry: String
        let openImage: String
        let playVideo: String
        let openFile: String
        let failed: String

        public init(retry: String, openImage: String, playVideo: String, openFile: String, failed: String) {
            self.retry = retry
            self.openImage = openImage
            self.playVideo = playVideo
            self.openFile = openFile
            self.failed = failed
        }
    }

    let titles: Titles
    let open: @MainActor (Int) async -> Bool

    /// - Parameter open: Opens the item at the index. Gives false when the host cannot read the bytes of the item.
    public init(titles: Titles, open: @escaping @MainActor (Int) async -> Bool) {
        self.titles = titles
        self.open = open
    }
}

/// Media of a product message, one item below the other, with optional text below them.
///
/// An image or a video that does not load shows a Retry button (02-DESIGN §6.2). A tap on an image, a video or a
/// file opens it through ``ChatMediaOpening``.
public struct ChatMediaStackView: View, Hashable {
    public static let reuseIdentifier = "ChatMediaStackView"

    fileprivate enum Layout {
        static let imageHeight: CGFloat = 200
        /// The bounds of the width of an image or a video, at the image height.
        static let minImageWidth: CGFloat = 120
        static let maxImageWidth: CGFloat = 260
    }

    let items: [ChatMediaStackItem]
    let text: ChatMarkdownText?
    let loader: ChatImageLoader
    /// Gives the poster of a video source.
    let posterLoader: ChatImageLoader
    let opening: ChatMediaOpening?

    public init(
        items: [ChatMediaStackItem],
        text: ChatMarkdownText?,
        loader: ChatImageLoader,
        posterLoader: ChatImageLoader,
        opening: ChatMediaOpening?
    ) {
        self.items = items
        self.text = text
        self.loader = loader
        self.posterLoader = posterLoader
        self.opening = opening
    }

    /// Images only, which the user cannot open.
    public init(sources: [String], alts: [String?] = [], text: ChatMarkdownText?, loader: ChatImageLoader) {
        let items = sources.enumerated().map { index, source in
            ChatMediaStackItem(source: source, kind: .image, alt: alts.indices.contains(index) ? alts[index] : nil)
        }
        self.init(items: items, text: text, loader: loader, posterLoader: loader, opening: nil)
    }

    /// The text alternative of each item, in the order of the items.
    public var alts: [String?] {
        items.map(\.alt)
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: DSSpacings.extraTiny) {
            ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                itemView(item, at: index)
            }

            if let text {
                text
                    .padding(.horizontal, DSSpacings.small + DSSpacings.tiny)
                    .padding(.vertical, DSSpacings.extraSmall)
            }
        }
    }

    @ViewBuilder
    private func itemView(_ item: ChatMediaStackItem, at index: Int) -> some View {
        switch item.kind {
        case .image,
             .video:
            let isVideo = item.kind == .video
            ChatRemoteMedia(
                source: item.source,
                isVideo: isVideo,
                loader: isVideo ? posterLoader : loader,
                titles: opening?.titles,
                onOpen: opening.map { opening in { await opening.open(index) } }
            )
            .frame(height: Layout.imageHeight)
            .clipped()
            .accessibilityLabel(Text(alt(at: index) ?? ""))
        case let .file(name, detail):
            ChatMediaFileRow(
                name: name,
                detail: detail,
                titles: opening?.titles,
                onOpen: opening.map { opening in { await opening.open(index) } }
            )
            .accessibilityLabel(Text(alt(at: index) ?? name))
        }
    }

    public static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.items == rhs.items && lhs.text == rhs.text && (lhs.opening == nil) == (rhs.opening == nil)
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(items)
        hasher.combine(text)
        hasher.combine(opening == nil)
    }

    /// The text alternative of the item at `index`, or nil when the product gave none.
    public func alt(at index: Int) -> String? {
        guard items.indices.contains(index), let alt = items[index].alt,
              !alt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return nil
        }
        return alt
    }
}

/// An image or the poster of a video. A tap opens it when it loaded. When it does not load, a Retry button asks for
/// the bytes again.
private struct ChatRemoteMedia: View {
    let source: String
    let isVideo: Bool
    let loader: ChatImageLoader
    let titles: ChatMediaOpening.Titles?
    let onOpen: (@MainActor () async -> Bool)?

    @State private var image: UIImage?
    @State private var failed = false
    /// Each Retry adds one, so the load runs again.
    @State private var attempt = 0

    private enum Layout {
        static let controlSize: CGFloat = 48
    }

    var body: some View {
        ZStack {
            Color(.bgSurfaceContainer)

            if let image {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFill()
                    .transition(.opacity)
            } else if !failed {
                ProgressView()
            }

            if failed {
                retryButton
            } else if isVideo, image != nil {
                Image(systemName: "play.fill")
                    .font(.system(size: 20, weight: .semibold))
                    .foregroundStyle(Color.white)
                    .frame(width: Layout.controlSize, height: Layout.controlSize)
                    .background(Color.black.opacity(0.5), in: Circle())
                    .accessibilityHidden(true)
            }
        }
        .frame(width: Self.width(of: image))
        .contentShape(Rectangle())
        .onTapGesture {
            guard image != nil, let onOpen else { return }
            Task { _ = await onOpen() }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityAddTraits(isVideo ? .startsMediaSession : .isImage)
        .accessibilityAddTraits(image != nil && onOpen != nil ? .isButton : [])
        .accessibilityHint(Text(hint))
        .animation(.easeInOut, value: image)
        .task(id: "\(attempt)#\(source)") {
            failed = false
            let loaded = await loader(source)
            image = loaded
            failed = loaded == nil
        }
    }

    /// The width at the image height, from the aspect of the image: a small preview draws as large as its file. An
    /// item that did not load yet is 4:3.
    static func width(of image: UIImage?) -> CGFloat {
        let height = ChatMediaStackView.Layout.imageHeight
        let aspect = image.flatMap { $0.size.height > 0 ? $0.size.width / $0.size.height : nil } ?? 4 / 3
        return min(
            max(height * aspect, ChatMediaStackView.Layout.minImageWidth),
            ChatMediaStackView.Layout.maxImageWidth
        )
    }

    private var hint: String {
        guard let titles else { return "" }
        if failed { return titles.retry }
        return isVideo ? titles.playVideo : titles.openImage
    }

    @ViewBuilder private var retryButton: some View {
        Button {
            attempt += 1
        } label: {
            Image(systemName: "arrow.clockwise")
                .font(.system(size: 18, weight: .semibold))
                .foregroundStyle(.fgPrimary)
                .frame(width: Layout.controlSize, height: Layout.controlSize)
                .background(Color(.bgSurfaceContainer).opacity(0.9), in: Circle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(Text(titles?.retry ?? ""))
    }
}

/// A file among the media: its icon, its name and its detail. A tap loads the file and opens it. A file that does
/// not load shows the failure, and the next tap tries again.
private struct ChatMediaFileRow: View {
    private enum State {
        case idle
        case loading
        case failed
    }

    let name: String
    let detail: String
    let titles: ChatMediaOpening.Titles?
    let onOpen: (@MainActor () async -> Bool)?

    @SwiftUI.State private var state = State.idle

    var body: some View {
        Button {
            guard let onOpen, state != .loading else { return }
            state = .loading
            Task {
                state = await onOpen() ? .idle : .failed
            }
        } label: {
            HStack(spacing: DSSpacings.small) {
                Group {
                    switch state {
                    case .loading: ProgressView()
                    case .failed: Image(systemName: "arrow.clockwise")
                    case .idle: Image(systemName: "doc")
                    }
                }
                .font(.system(size: 20))
                .foregroundStyle(.fgPrimary)
                .frame(width: 24, height: 24)

                VStack(alignment: .leading, spacing: 2) {
                    Text(name)
                        .scaledTypography(.bodyMediumEmphasized)
                        .foregroundStyle(.fgPrimary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    Text(state == .failed ? titles?.failed ?? detail : detail)
                        .scaledTypography(.bodySmall)
                        .foregroundStyle(.fgSecondary)
                        .lineLimit(2)
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, DSSpacings.small + DSSpacings.tiny)
            .padding(.vertical, DSSpacings.small)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(onOpen == nil)
        .accessibilityHint(Text(titles?.openFile ?? ""))
    }
}
