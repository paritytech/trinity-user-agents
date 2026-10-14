import Foundation
import SubstrateSdk

extension Chat.LocalMessage.Content {
    struct RichText: Equatable {
        let text: String?
        let attachments: [Attachment]?
    }

    enum Attachment: Equatable {
        case remoteDownloadable(ChatRemoteMessageContent.FileVariant)
        case localUploadable(LocalUploadableFile)
    }

    struct LocalUploadableFile: Equatable {
        // for example, actual preprocessed image is stored in tmp directory
        // and in this case it is a relative path to that
        let relativeLocalPath: String
        let meta: ChatRemoteMessageContent.FileMeta
        let uploadingInfo: FileUploadingInfo?

        var isUploaded: Bool {
            uploadingInfo != nil
        }
    }

    enum FileUploadingInfo: Equatable {
        struct ToPeer: Equatable {
            let identifier: Data
            let claimTicket: Data
            let node: ChatRemoteMessageContent.NodeEndpoint
        }

        case toPeer(ToPeer)
    }
}

extension Chat.LocalMessage.Content.RichText {
    init(remoteRichText: ChatRemoteMessageContent.RichText) {
        text = remoteRichText.text

        attachments = remoteRichText.attachments?.map { .remoteDownloadable($0) }
    }

    var isReadyToSend: Bool {
        guard let attachments, !attachments.isEmpty else {
            // allow to send if no attachments
            return true
        }

        return attachments.allSatisfy { attachment in
            switch attachment {
            case let .localUploadable(localUploadable):
                localUploadable.uploadingInfo != nil
            case .remoteDownloadable:
                true
            }
        }
    }

    func toRemote() -> ChatRemoteMessageContent.RichText? {
        guard let attachments, !attachments.isEmpty else {
            return ChatRemoteMessageContent.RichText(text: text, attachments: nil)
        }

        var remoteAttachments: [ChatRemoteMessageContent.FileVariant] = []

        for attachment in attachments {
            switch attachment {
            case let .remoteDownloadable(downloadableFile):
                remoteAttachments.append(downloadableFile)
            case let .localUploadable(uploadableFile):
                guard let remoteAttachment = uploadableFile.toRemoteFileVariant() else {
                    // some of the attachments still being process
                    // so we don't allow the whole message sent
                    return nil
                }

                remoteAttachments.append(remoteAttachment)
            }
        }

        return ChatRemoteMessageContent.RichText(
            text: text,
            attachments: remoteAttachments
        )
    }
}

extension Chat.LocalMessage.Content.LocalUploadableFile {
    func toRemoteFileVariant() -> ChatRemoteMessageContent.FileVariant? {
        guard case let .toPeer(uploadingInfo) = uploadingInfo else {
            return nil
        }

        let file = ChatRemoteMessageContent.P2PMixnetFile(
            identifier: uploadingInfo.identifier,
            claimTicket: uploadingInfo.claimTicket,
            node: uploadingInfo.node,
            meta: meta
        )

        return .p2pMixnet(file)
    }
}

// MARK: - ScaleCodable

extension Chat.LocalMessage.Content.RichText: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        text = try ScaleOption<String>(scaleDecoder: scaleDecoder).value
        attachments = try ScaleOption<[Chat.LocalMessage.Content.Attachment]>(
            scaleDecoder: scaleDecoder
        ).value
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try ScaleOption(value: text).encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: attachments).encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.LocalMessage.Content.Attachment: ScaleCodable {
    private var scaleIndex: UInt8 {
        switch self {
        case .remoteDownloadable: 0
        case .localUploadable: 1
        }
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        let idx = try UInt8(scaleDecoder: scaleDecoder)
        switch idx {
        case 0:
            self = try .remoteDownloadable(
                ChatRemoteMessageContent.FileVariant(
                    scaleDecoder: scaleDecoder
                )
            )
        case 1:
            self = try .localUploadable(
                Chat.LocalMessage.Content.LocalUploadableFile(
                    scaleDecoder: scaleDecoder
                )
            )
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try scaleIndex.encode(scaleEncoder: scaleEncoder)
        switch self {
        case let .remoteDownloadable(model):
            try model.encode(scaleEncoder: scaleEncoder)
        case let .localUploadable(model):
            try model.encode(scaleEncoder: scaleEncoder)
        }
    }
}

extension Chat.LocalMessage.Content.LocalUploadableFile: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        relativeLocalPath = try String(scaleDecoder: scaleDecoder)
        meta = try ChatRemoteMessageContent.FileMeta(scaleDecoder: scaleDecoder)
        uploadingInfo = try ScaleOption<Chat.LocalMessage.Content.FileUploadingInfo>(
            scaleDecoder: scaleDecoder
        ).value
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try relativeLocalPath.encode(scaleEncoder: scaleEncoder)
        try meta.encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: uploadingInfo).encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.LocalMessage.Content.FileUploadingInfo: ScaleCodable {
    private var scaleIndex: UInt8 {
        switch self {
        case .toPeer: 0
        }
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        let idx = try UInt8(scaleDecoder: scaleDecoder)
        switch idx {
        case 0:
            self = try .toPeer(ToPeer(scaleDecoder: scaleDecoder))
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try scaleIndex.encode(scaleEncoder: scaleEncoder)
        switch self {
        case let .toPeer(model):
            try model.encode(scaleEncoder: scaleEncoder)
        }
    }
}

extension Chat.LocalMessage.Content.FileUploadingInfo.ToPeer: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        identifier = try Data(scaleDecoder: scaleDecoder)
        claimTicket = try Data(scaleDecoder: scaleDecoder)
        node = try ChatRemoteMessageContent.NodeEndpoint(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try identifier.encode(scaleEncoder: scaleEncoder)
        try claimTicket.encode(scaleEncoder: scaleEncoder)
        try node.encode(scaleEncoder: scaleEncoder)
    }
}
