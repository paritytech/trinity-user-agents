import Foundation
import SubstrateSdk

extension Chat.RemoteMessageContentV1.MessageContent {
    struct RichText: Equatable {
        let text: String?
        let attachments: [FileVariant]?
    }

    enum FileVariant: Equatable {
        case p2pMixnet(P2PMixnetFile)
    }

    enum FileMeta: Equatable {
        case general(GeneralFileMeta)
        case image(ImageFileMeta)
        case video(VideoFileMeta)

        var mimeType: String {
            switch self {
            case let .general(meta):
                meta.mimeType
            case let .image(meta):
                meta.general.mimeType
            case let .video(meta):
                meta.general.mimeType
            }
        }

        var fileSize: UInt32 {
            switch self {
            case let .general(meta):
                meta.fileSize
            case let .image(meta):
                meta.general.fileSize
            case let .video(meta):
                meta.general.fileSize
            }
        }
    }

    struct ImageFileMeta: Equatable {
        let general: GeneralFileMeta
        let width: UInt32
        let height: UInt32
        /// UTF-8 encoded BlurHash used as the image's pre-download preview.
        let thumbnail: Data?
    }

    struct VideoFileMeta: Equatable {
        let general: GeneralFileMeta
        let duration: UInt32
        /// UTF-8 encoded BlurHash of a video frame used as the pre-download preview.
        let thumbnail: Data?
    }

    struct GeneralFileMeta: Equatable {
        let mimeType: String
        let fileSize: UInt32
    }

    struct P2PMixnetFile: Equatable {
        let identifier: Data
        let claimTicket: Data
        let node: NodeEndpoint
        let meta: FileMeta
    }

    enum NodeEndpoint: Hashable {
        case wssUrl(String)
    }
}

extension Chat.RemoteMessageContentV1.MessageContent.RichText: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        text = try ScaleOption<String>(scaleDecoder: scaleDecoder).value
        attachments = try ScaleOption<[Chat.RemoteMessageContentV1.MessageContent.FileVariant]>(
            scaleDecoder: scaleDecoder
        ).value
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try ScaleOption(value: text).encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: attachments).encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RemoteMessageContentV1.MessageContent.FileVariant: ScaleCodable {
    private var scaleIndex: UInt8 {
        switch self {
        case .p2pMixnet: 0
        }
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        let idx = try UInt8(scaleDecoder: scaleDecoder)
        switch idx {
        case 0:
            self = try .p2pMixnet(
                Chat.RemoteMessageContentV1.MessageContent.P2PMixnetFile(scaleDecoder: scaleDecoder)
            )
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try scaleIndex.encode(scaleEncoder: scaleEncoder)
        switch self {
        case let .p2pMixnet(model):
            try model.encode(scaleEncoder: scaleEncoder)
        }
    }
}

extension Chat.RemoteMessageContentV1.MessageContent.P2PMixnetFile: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        identifier = try Data(scaleDecoder: scaleDecoder)
        claimTicket = try Data(scaleDecoder: scaleDecoder)
        node = try Chat.RemoteMessageContentV1.MessageContent.NodeEndpoint(scaleDecoder: scaleDecoder)
        meta = try Chat.RemoteMessageContentV1.MessageContent.FileMeta(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try identifier.encode(scaleEncoder: scaleEncoder)
        try claimTicket.encode(scaleEncoder: scaleEncoder)
        try node.encode(scaleEncoder: scaleEncoder)
        try meta.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RemoteMessageContentV1.MessageContent.NodeEndpoint: ScaleCodable {
    private var scaleIndex: UInt8 {
        switch self {
        case .wssUrl: 0
        }
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        let idx = try UInt8(scaleDecoder: scaleDecoder)
        switch idx {
        case 0:
            self = try .wssUrl(String(scaleDecoder: scaleDecoder))
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try scaleIndex.encode(scaleEncoder: scaleEncoder)
        switch self {
        case let .wssUrl(urlString):
            try urlString.encode(scaleEncoder: scaleEncoder)
        }
    }
}

extension Chat.RemoteMessageContentV1.MessageContent.FileMeta: ScaleCodable {
    private var scaleIndex: UInt8 {
        switch self {
        case .general: 0
        case .image: 1
        case .video: 2
        }
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        let idx = try UInt8(scaleDecoder: scaleDecoder)

        switch idx {
        case 0:
            let general = try Chat.RemoteMessageContentV1.MessageContent.GeneralFileMeta(
                scaleDecoder: scaleDecoder
            )

            self = .general(general)
        case 1:
            let image = try Chat.RemoteMessageContentV1.MessageContent.ImageFileMeta(
                scaleDecoder: scaleDecoder
            )

            self = .image(image)
        case 2:
            let video = try Chat.RemoteMessageContentV1.MessageContent.VideoFileMeta(
                scaleDecoder: scaleDecoder
            )

            self = .video(video)
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try scaleIndex.encode(scaleEncoder: scaleEncoder)

        switch self {
        case let .general(meta):
            try meta.encode(scaleEncoder: scaleEncoder)
        case let .image(meta):
            try meta.encode(scaleEncoder: scaleEncoder)
        case let .video(meta):
            try meta.encode(scaleEncoder: scaleEncoder)
        }
    }
}

extension Chat.RemoteMessageContentV1.MessageContent.GeneralFileMeta: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        mimeType = try String(scaleDecoder: scaleDecoder)
        fileSize = try UInt32(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try mimeType.encode(scaleEncoder: scaleEncoder)
        try fileSize.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RemoteMessageContentV1.MessageContent.ImageFileMeta: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        general = try Chat.RemoteMessageContentV1.MessageContent.GeneralFileMeta(scaleDecoder: scaleDecoder)
        width = try UInt32(scaleDecoder: scaleDecoder)
        height = try UInt32(scaleDecoder: scaleDecoder)
        thumbnail = try ScaleOption<Data>(scaleDecoder: scaleDecoder).value
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try general.encode(scaleEncoder: scaleEncoder)
        try width.encode(scaleEncoder: scaleEncoder)
        try height.encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: thumbnail).encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RemoteMessageContentV1.MessageContent.VideoFileMeta: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        general = try Chat.RemoteMessageContentV1.MessageContent.GeneralFileMeta(scaleDecoder: scaleDecoder)
        duration = try UInt32(scaleDecoder: scaleDecoder)
        thumbnail = try ScaleOption<Data>(scaleDecoder: scaleDecoder).value
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try general.encode(scaleEncoder: scaleEncoder)
        try duration.encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: thumbnail).encode(scaleEncoder: scaleEncoder)
    }
}
