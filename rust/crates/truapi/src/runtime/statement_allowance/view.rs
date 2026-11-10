//! Runtime view-function reads for allowance allocation.

use parity_scale_codec::{Decode, DecodeAll, Encode};
use scale_info::{TypeDef, TypeDefPrimitive};
use serde_json::Value;
use thiserror::Error;

use super::{
	StatementAllowanceError, extension::Metadata, rpc::RpcClient, view_cache::MetadataViewCache,
};

const EXECUTE_VIEW_FUNCTION: &str = "RuntimeViewFunction_execute_view_function";

/// Runtime view-function failure.
#[derive(Debug, Error)]
#[error(transparent)]
pub struct ViewFunctionError(ViewFunctionFailure);

#[derive(Debug, Error)]
enum ViewFunctionFailure {
	#[error("{pallet}.{function} state_call returned a non-string response")]
	ResultNotString { pallet: &'static str, function: &'static str },
	#[error("{pallet}.{function} response hex: {source}")]
	ResponseHex {
		pallet: &'static str,
		function: &'static str,
		#[source]
		source: hex::FromHexError,
	},
	#[error("{pallet}.{function} response decode: {source}")]
	ResponseDecode {
		pallet: &'static str,
		function: &'static str,
		#[source]
		source: parity_scale_codec::Error,
	},
	#[error("{pallet}.{function} response has {remaining} trailing bytes")]
	ResponseTrailingBytes { pallet: &'static str, function: &'static str, remaining: usize },
	#[error("{pallet}.{function} dispatch failed: {reason}")]
	Dispatch { pallet: &'static str, function: &'static str, reason: String },
	#[error("{pallet}.{function} view function missing")]
	Missing { pallet: &'static str, function: &'static str },
	#[error("{pallet}.{function} declares {actual} inputs, expected none")]
	Inputs { pallet: &'static str, function: &'static str, actual: usize },
	#[error("{pallet}.{function} output type {type_id} is not an unsigned integer up to u32")]
	OutputType { pallet: &'static str, function: &'static str, type_id: u32 },
	#[error("cannot decode {pallet}.{function} output as {width}: {source}")]
	OutputDecode {
		pallet: &'static str,
		function: &'static str,
		width: &'static str,
		#[source]
		source: parity_scale_codec::Error,
	},
}

#[derive(Debug, Decode, Encode)]
struct ViewFunctionId {
	prefix: [u8; 16],
	suffix: [u8; 16],
}

#[derive(Debug, Decode, Encode)]
enum ViewFunctionDispatchError {
	NotImplemented,
	NotFound(ViewFunctionId),
	Codec,
}

async fn read_u32(
	rpc: &RpcClient,
	metadata: &Metadata,
	pallet: &'static str,
	function: &'static str,
	at: Option<&str>,
) -> Result<u32, StatementAllowanceError> {
	let definition = metadata
		.view_function(pallet, function)
		.ok_or_else(|| view_error(ViewFunctionFailure::Missing { pallet, function }))?;
	if definition.inputs != 0 {
		return Err(view_error(ViewFunctionFailure::Inputs {
			pallet,
			function,
			actual: definition.inputs,
		}));
	}
	let id = definition.id;
	if at.is_none() &&
		let Some(value) = metadata.cached_view_u32(&id)
	{
		return Ok(value);
	}

	let output = execute_no_args(rpc, pallet, function, id, at).await?;
	let primitive = metadata
		.registry()
		.resolve(definition.output_type)
		.and_then(|ty| match &ty.type_def {
			TypeDef::Primitive(primitive) => Some(primitive),
			_ => None,
		})
		.ok_or_else(|| {
			view_error(ViewFunctionFailure::OutputType {
				pallet,
				function,
				type_id: definition.output_type,
			})
		})?;
	macro_rules! decode {
		($type:ty, $width:literal) => {
			<$type>::decode_all(&mut &output[..]).map(u32::from).map_err(|source| {
				view_error(ViewFunctionFailure::OutputDecode {
					pallet,
					function,
					width: $width,
					source,
				})
			})
		};
	}
	let value = match primitive {
		TypeDefPrimitive::U8 => decode!(u8, "u8"),
		TypeDefPrimitive::U16 => decode!(u16, "u16"),
		TypeDefPrimitive::U32 => decode!(u32, "u32"),
		_ => Err(view_error(ViewFunctionFailure::OutputType {
			pallet,
			function,
			type_id: definition.output_type,
		})),
	}?;
	if at.is_none() {
		metadata.cache_view_u32(id, value);
	}
	Ok(value)
}

pub async fn read_resource_u32(
	rpc: &RpcClient,
	metadata: &Metadata,
	function: &'static str,
) -> Result<u32, StatementAllowanceError> {
	read_u32(rpc, metadata, "Resources", function, None).await
}

/// Resolve a runtime policy against the same block as an inspection's state.
pub(super) async fn read_resource_u32_at(
	rpc: &RpcClient,
	metadata: &Metadata,
	function: &'static str,
	at: &str,
) -> Result<u32, StatementAllowanceError> {
	read_u32(rpc, metadata, "Resources", function, Some(at)).await
}

pub fn supports_resource_u32(metadata: &Metadata, function: &'static str) -> bool {
	let Some(definition) = metadata.view_function("Resources", function) else {
		return false;
	};
	definition.inputs == 0 &&
		matches!(
			metadata.registry().resolve(definition.output_type).map(|ty| &ty.type_def),
			Some(TypeDef::Primitive(
				TypeDefPrimitive::U8 | TypeDefPrimitive::U16 | TypeDefPrimitive::U32
			))
		)
}

async fn execute_no_args(
	rpc: &RpcClient,
	pallet: &'static str,
	function: &'static str,
	id: [u8; 32],
	at: Option<&str>,
) -> Result<Vec<u8>, StatementAllowanceError> {
	let mut arguments = id.to_vec();
	Vec::<u8>::new().encode_to(&mut arguments);
	let mut params = vec![
		Value::String(EXECUTE_VIEW_FUNCTION.to_string()),
		Value::String(format!("0x{}", hex::encode(arguments))),
	];
	if let Some(at) = at {
		params.push(Value::String(at.to_string()));
	}
	let response = rpc.call("state_call", Value::Array(params)).await?;
	ViewFunctionError::decode_response(pallet, function, response)
}

impl ViewFunctionError {
	/// Decode the SCALE dispatch result returned by a runtime view-function call.
	pub fn decode_response(
		pallet: &'static str,
		function: &'static str,
		response: Value,
	) -> Result<Vec<u8>, StatementAllowanceError> {
		let encoded = response
			.as_str()
			.ok_or_else(|| view_error(ViewFunctionFailure::ResultNotString { pallet, function }))?;
		let bytes =
			hex::decode(encoded.strip_prefix("0x").unwrap_or(encoded)).map_err(|source| {
				view_error(ViewFunctionFailure::ResponseHex { pallet, function, source })
			})?;
		let mut cursor = &bytes[..];
		let dispatched = Result::<Vec<u8>, ViewFunctionDispatchError>::decode(&mut cursor)
			.map_err(|source| {
				view_error(ViewFunctionFailure::ResponseDecode { pallet, function, source })
			})?;
		if !cursor.is_empty() {
			return Err(view_error(ViewFunctionFailure::ResponseTrailingBytes {
				pallet,
				function,
				remaining: cursor.len(),
			}));
		}
		dispatched.map_err(|reason| {
			view_error(ViewFunctionFailure::Dispatch {
				pallet,
				function,
				reason: format!("{reason:?}"),
			})
		})
	}
}

fn view_error(error: ViewFunctionFailure) -> StatementAllowanceError {
	ViewFunctionError(error).into()
}

#[cfg(test)]
mod tests;
