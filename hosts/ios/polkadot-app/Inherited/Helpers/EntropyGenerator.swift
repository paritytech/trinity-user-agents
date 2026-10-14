import Foundation
import Security

enum EntropyError: Error {
    case generationFailed(status: OSStatus)

    var localizedDescription: String {
        switch self {
        case let .generationFailed(status):
            if let errorMessage = SecCopyErrorMessageString(status, nil) as String? {
                errorMessage
            } else {
                "Unknown error occurred with status code: \(status)"
            }
        }
    }
}

protocol EntropyGenerating: AnyObject {
    func generateEntropy(of size: Int) -> Result<Data, EntropyError>
}

final class EntropyGenerator: EntropyGenerating {
    func generateEntropy(of size: Int) -> Result<Data, EntropyError> {
        var data = Data(count: size)
        let result = data.withUnsafeMutableBytes { mutableBytes in
            guard let baseAddress = mutableBytes.baseAddress else {
                return errSecAllocate
            }
            return SecRandomCopyBytes(kSecRandomDefault, size, baseAddress)
        }
        if result == errSecSuccess {
            return .success(data)
        } else {
            return .failure(EntropyError.generationFailed(status: result))
        }
    }
}
