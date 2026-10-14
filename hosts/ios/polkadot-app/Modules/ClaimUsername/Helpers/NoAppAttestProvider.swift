#if DISABLE_AUTH
    import Foundation
    import Operation_iOS
    import UniqueDevice

    final class NoAppAttestProvider: AppAttestProviding {
        func setup() {}

        func appAttestModifier(
            for bodyDataClosure: @escaping () throws -> Data?,
            clientIdClosure _: (() throws -> Data)?
        ) -> CompoundOperationWrapper<HttpRequestModifier> {
            appAttestModifier(for: bodyDataClosure)
        }
    }

    private extension NoAppAttestProvider {
        func appAttestModifier(
            for closure: @escaping () throws -> Data?
        ) -> Operation_iOS.CompoundOperationWrapper<any HttpRequestModifier> {
            let operation: BaseOperation<any HttpRequestModifier> = ClosureOperation {
                try Modifier(data: closure())
            }
            return CompoundOperationWrapper(targetOperation: operation)
        }

        final class Modifier: HttpRequestModifier {
            let data: Data?
            init(data: Data?) {
                self.data = data
            }

            func visit(request: inout URLRequest) throws {
                request.httpBody = data
            }
        }
    }
#endif
