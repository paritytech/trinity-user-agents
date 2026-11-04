//! Typed contract results for scripted runtime responses.

use super::*;

/// Encodes a contract response with distinct consumed and required limits.
pub fn contract_output(flags: u32, data: Vec<u8>) -> Vec<u8> {
	contract_result(Ok(ExecReturnValue { flags, data })).encode()
}

/// Encodes a runtime rejection before contract execution.
pub fn contract_dispatch_error(error: Vec<u8>) -> Vec<u8> {
	contract_result(Err(RawDispatchError(error))).encode()
}

fn contract_result(result: Result<ExecReturnValue, RawDispatchError>) -> ContractResult {
	ContractResult {
		_weight_consumed: Weight { ref_time: 10, proof_size: 20 },
		weight_required: Weight { ref_time: 300, proof_size: 400 },
		storage_deposit: StorageDeposit::Refund { _amount: 500 },
		max_storage_deposit: StorageDeposit::Charge(600),
		_gas_consumed: 70,
		result,
	}
}
