import BulletinChain
import Foundation

actor MockPreimageFetcher: IpfsFetching {
    enum Failure: Error {
        case offline
    }

    private var result: Result<Data, Failure>
    private(set) var requestedHashes: [Data] = []

    init(result: Result<Data, Failure>) {
        self.result = result
    }

    func lookupBy(rawHash: Data) throws -> Data {
        requestedHashes.append(rawHash)
        return try result.get()
    }

    func lookupBy(cid _: String) throws -> Data {
        try result.get()
    }

    func setResult(_ result: Result<Data, Failure>) {
        self.result = result
    }
}
