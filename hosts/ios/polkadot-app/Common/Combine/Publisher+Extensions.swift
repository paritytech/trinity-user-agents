import Foundation
import Combine

extension Publisher {
    func delayAtLeast(
        for interval: TimeInterval
    ) -> AnyPublisher<Output, Failure> {
        Deferred {
            let date: DispatchTime = .now()
            return self
                .map {
                    Result<Output, Failure>.success($0)
                }
                .catch {
                    Just(Result<Output, Failure>.failure($0))
                }
                .flatMap {
                    let diffNanoseconds = DispatchTime.now().uptimeNanoseconds - date.uptimeNanoseconds
                    let diffSeconds = TimeInterval(diffNanoseconds) / 1_000_000_000
                    let remaining = Swift.max(interval - diffSeconds, 0)

                    let retVal = Just($0).setFailureType(to: Failure.self)

                    guard remaining > 0 else {
                        return retVal.eraseToAnyPublisher()
                    }
                    return retVal
                        .delay(for: .seconds(remaining), scheduler: DispatchQueue.main)
                        .eraseToAnyPublisher()
                }
                .tryMap { result in
                    try result.get()
                }
                // swiftlint:disable:next force_cast
                .mapError { $0 as! Failure }
                .eraseToAnyPublisher()
        }
        .eraseToAnyPublisher()
    }
}
