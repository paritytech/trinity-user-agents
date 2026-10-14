import Foundation

enum BarcodeCreationError: Error {
    case generatorUnavailable
    case generatedImageInvalid
    case bitmapImageCreationFailed
}
