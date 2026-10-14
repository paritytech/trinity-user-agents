import Foundation
import Operation_iOS
import SubstrateSdk
import SubstrateStateCall
import SubstrateSdkExt
import ChainStore
import StructuredConcurrency

public protocol ViewFunctionExecuting {
    func call<T: Decodable>(
        viewFunction: ViewFunctionCodingPath,
        chainId: ChainId,
        args: [ScaleEncodable]
    ) async throws -> T
}

public extension ViewFunctionExecuting {
    func call<T: Decodable>(
        viewFunction: ViewFunctionCodingPath,
        chainId: ChainId
    ) async throws -> T {
        try await call(viewFunction: viewFunction, chainId: chainId, args: [])
    }
}

public enum ViewFunctionExecutorError: Error {
    case functionNotFound(ViewFunctionCodingPath)
    case unexpectedInputsCount(expected: Int, actual: Int)
    case execution(JSON)
}

public final class ViewFunctionExecutor: SubstrateRuntimeApiOperationFactory {}

extension ViewFunctionExecutor: ViewFunctionExecuting {
    public func call<T: Decodable>(
        viewFunction: ViewFunctionCodingPath,
        chainId: ChainId,
        args: [ScaleEncodable]
    ) async throws -> T {
        let runtimeProvider = try chainRegistry.getRuntimeCodingServiceOrError(for: chainId)

        let codingFactory = try await runtimeProvider.fetchCoderFactoryOperation().asyncExecute()

        guard let query = codingFactory.metadata.getViewFunction(using: viewFunction) else {
            throw ViewFunctionExecutorError.functionNotFound(viewFunction)
        }

        guard query.function.inputs.count == args.count else {
            throw ViewFunctionExecutorError.unexpectedInputsCount(
                expected: query.function.inputs.count,
                actual: args.count
            )
        }

        let result: Substrate.Result<BytesCodable, JSON> = try await createRuntimeCallWrapper(
            for: chainId,
            path: StateCallPath(
                module: ViewFunctionQueryResult.executeApiName,
                method: ViewFunctionQueryResult.executeMethodName
            )
        ) { _, encoder, _ in
            try encoder.appendRawData(query.functionId)

            let encodedArgs = try args.reduce(ScaleEncoder()) { argsEncoder, arg in
                try arg.encode(scaleEncoder: argsEncoder)
                return argsEncoder
            }.encode()

            try encoder.append(encodable: encodedArgs)
        }
        .asyncExecute()

        let data = try result.ensureOkOrError { reason in
            ViewFunctionExecutorError.execution(reason)
        }.wrappedValue

        return try codingFactory
            .createDecoder(from: data)
            .read(
                of: query.function.output.asTypeId(),
                with: codingFactory.createRuntimeJsonContext().toRawContext()
            )
    }
}
