import Foundation

protocol ChatAttachmentProviding {
    var neededAudioActivity: AudioSessionActivity? { get }

    func prepareForSend(using store: AttachmentStoring) async throws -> ProcessedAttachment
}
