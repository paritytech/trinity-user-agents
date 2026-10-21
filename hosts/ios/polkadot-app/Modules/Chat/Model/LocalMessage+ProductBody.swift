import Foundation
import SubstrateSdk

extension Chat.LocalMessage.Content {
    /// A body that a product posts into its own room and the host draws with native views.
    enum ProductBody: Equatable {
        case richText(ProductRichText)
        case actions(ProductActions)
        case file(ProductFile)
        /// A line about the room, which the room draws as a system row (`ChatMessageBody::Notice`).
        case notice(String)
    }

    /// Text with media. Each media item is an `https` URL or an inline `data:` image.
    struct ProductRichText: Equatable {
        let text: String?
        let media: [String]
        /// The text alternative of each media item, in the order of `media`, for VoiceOver (P7.2). Empty when no
        /// item has one, and for a message that a host stored before it kept them.
        let alts: [String?]
        /// The kind of each media item, in the order of `media`. Empty for a message that a host stored before it
        /// kept them. Then ``info(at:)`` reads the kind from the source.
        let items: [ProductMediaInfo]

        init(text: String?, media: [String], alts: [String?] = [], items: [ProductMediaInfo] = []) {
            self.text = text
            self.media = media
            self.alts = alts.contains { $0 != nil } ? alts : []
            self.items = items.count == media.count ? items : []
        }

        /// The text alternative of the media item at `index`.
        func alt(at index: Int) -> String? {
            alts.indices.contains(index) ? alts[index] : nil
        }

        /// The kind of the media item at `index`. Without a stored kind, an inline `video/` source is a video, and
        /// any other source is an image.
        func info(at index: Int) -> ProductMediaInfo {
            if items.indices.contains(index) {
                return items[index]
            }

            let source = media.indices.contains(index) ? media[index] : ""
            let isVideo = source.prefix(16).lowercased().hasPrefix("data:video/")
            return ProductMediaInfo(kind: isVideo ? .video : .image, mimeType: "", fileName: nil, sizeBytes: nil)
        }
    }

    /// What the host knows of one media item of a product body, beside its source (02-DESIGN §6.2).
    struct ProductMediaInfo: Equatable {
        enum Kind: UInt8, Equatable {
            case image = 0
            case video = 1
            case file = 2
        }

        let kind: Kind
        let mimeType: String
        /// The name of a file. Nil for an image and a video.
        let fileName: String?
        let sizeBytes: UInt64?
    }

    struct ProductActions: Equatable {
        let text: String?
        let buttons: [ProductActionButton]
        let layout: ProductActionLayout
    }

    struct ProductActionButton: Equatable {
        let actionId: String
        let title: String
    }

    enum ProductActionLayout: UInt8, Equatable {
        case column = 0
        case grid = 1
    }

    /// A file that the product serves at an `https` URL.
    struct ProductFile: Equatable {
        let url: String
        let fileName: String
        let mimeType: String
        let sizeBytes: UInt64
        let text: String?
    }
}

extension Chat.LocalMessage.Content.ProductBody {
    /// The text part of the body, or nil when the body has no text.
    var text: String? {
        switch self {
        case let .richText(richText): richText.text
        case let .actions(actions): actions.text
        case let .file(file): file.text
        case let .notice(text): text
        }
    }
}

// MARK: - ScaleCodable

extension Chat.LocalMessage.Content.ProductBody: ScaleCodable {
    private enum Tag: UInt8 {
        case richText = 0
        case actions = 1
        case file = 2
        case notice = 3
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        let rawTag = try UInt8(scaleDecoder: scaleDecoder)
        switch Tag(rawValue: rawTag) {
        case .richText:
            self = try .richText(.init(scaleDecoder: scaleDecoder))
        case .actions:
            self = try .actions(.init(scaleDecoder: scaleDecoder))
        case .file:
            self = try .file(.init(scaleDecoder: scaleDecoder))
        case .notice:
            self = try .notice(String(scaleDecoder: scaleDecoder))
        case .none:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case let .richText(content):
            try Tag.richText.rawValue.encode(scaleEncoder: scaleEncoder)
            try content.encode(scaleEncoder: scaleEncoder)
        case let .actions(content):
            try Tag.actions.rawValue.encode(scaleEncoder: scaleEncoder)
            try content.encode(scaleEncoder: scaleEncoder)
        case let .file(content):
            try Tag.file.rawValue.encode(scaleEncoder: scaleEncoder)
            try content.encode(scaleEncoder: scaleEncoder)
        case let .notice(text):
            try Tag.notice.rawValue.encode(scaleEncoder: scaleEncoder)
            try text.encode(scaleEncoder: scaleEncoder)
        }
    }
}

extension Chat.LocalMessage.Content.ProductRichText: ScaleCodable {
    /// The text alternatives come last, after the fields of the first layout. The body is the last value of the
    /// stored content, so a row that a host stored before has no bytes left for them.
    /// The kinds of the media items come after the text alternatives, for the same reason (PARITY row 4).
    init(scaleDecoder: any ScaleDecoding) throws {
        text = try ScaleOption<String>(scaleDecoder: scaleDecoder).value
        media = try [String](scaleDecoder: scaleDecoder)
        let alts = scaleDecoder.remained > 0
            ? try [ScaleOption<String>](scaleDecoder: scaleDecoder).map(\.value)
            : []
        self.alts = alts.contains { $0 != nil } ? alts : []
        let items = scaleDecoder.remained > 0
            ? try [Chat.LocalMessage.Content.ProductMediaInfo](scaleDecoder: scaleDecoder)
            : []
        self.items = items.count == media.count ? items : []
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try ScaleOption(value: text).encode(scaleEncoder: scaleEncoder)
        try media.encode(scaleEncoder: scaleEncoder)
        try alts.map { ScaleOption(value: $0) }.encode(scaleEncoder: scaleEncoder)
        try items.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.LocalMessage.Content.ProductMediaInfo: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let rawKind = try UInt8(scaleDecoder: scaleDecoder)
        guard let kind = Kind(rawValue: rawKind) else {
            throw ScaleCodingError.unexpectedDecodedValue
        }
        self.kind = kind
        mimeType = try String(scaleDecoder: scaleDecoder)
        fileName = try ScaleOption<String>(scaleDecoder: scaleDecoder).value
        sizeBytes = try ScaleOption<UInt64>(scaleDecoder: scaleDecoder).value
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try kind.rawValue.encode(scaleEncoder: scaleEncoder)
        try mimeType.encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: fileName).encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: sizeBytes).encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.LocalMessage.Content.ProductActions: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        text = try ScaleOption<String>(scaleDecoder: scaleDecoder).value
        buttons = try [Chat.LocalMessage.Content.ProductActionButton](scaleDecoder: scaleDecoder)
        let rawLayout = try UInt8(scaleDecoder: scaleDecoder)
        guard let layout = Chat.LocalMessage.Content.ProductActionLayout(rawValue: rawLayout) else {
            throw ScaleCodingError.unexpectedDecodedValue
        }
        self.layout = layout
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try ScaleOption(value: text).encode(scaleEncoder: scaleEncoder)
        try buttons.encode(scaleEncoder: scaleEncoder)
        try layout.rawValue.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.LocalMessage.Content.ProductActionButton: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        actionId = try String(scaleDecoder: scaleDecoder)
        title = try String(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try actionId.encode(scaleEncoder: scaleEncoder)
        try title.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.LocalMessage.Content.ProductFile: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        url = try String(scaleDecoder: scaleDecoder)
        fileName = try String(scaleDecoder: scaleDecoder)
        mimeType = try String(scaleDecoder: scaleDecoder)
        sizeBytes = try UInt64(scaleDecoder: scaleDecoder)
        text = try ScaleOption<String>(scaleDecoder: scaleDecoder).value
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try url.encode(scaleEncoder: scaleEncoder)
        try fileName.encode(scaleEncoder: scaleEncoder)
        try mimeType.encode(scaleEncoder: scaleEncoder)
        try sizeBytes.encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: text).encode(scaleEncoder: scaleEncoder)
    }
}
