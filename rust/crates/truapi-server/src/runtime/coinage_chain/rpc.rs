// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer core/crates/brevity-ffi/src/{coinage_sender,coinage_transfer,game_submitter}.rs.
// Copyright the Brevity contributors. See truapi-coinage/NOTICE and LICENSE.

use super::HostCoinageChain;
use super::transaction::{Snapshot, decode_exact, hex0x, parse_hash};
use core::time::Duration;
use futures::FutureExt;
use parity_scale_codec::Encode;
use scale_info::{TypeDef, TypeDefPrimitive};
use serde_json::{Value, json};
use std::collections::HashMap;
use subxt_rpcs::{
    RpcClient,
    client::{RpcParams, rpc_params},
};

pub(super) async fn call(rpc: &RpcClient, method: &str, params: Value) -> Result<Value, String> {
    let mut encoded = RpcParams::new();
    for value in params.as_array().ok_or("invalid Host RPC parameters")? {
        encoded
            .push(value)
            .map_err(|_| "invalid Host RPC parameter")?;
    }
    let request = rpc.request(method, encoded).fuse();
    let timeout = futures_timer::Delay::new(Duration::from_secs(30)).fuse();
    futures::pin_mut!(request, timeout);
    futures::select! {
        result = request => result.map_err(|_| format!("Coinage chain request failed ({method})")),
        _ = timeout => Err(format!("Coinage chain request timed out ({method})")),
    }
}

pub(super) async fn free_unload_token_limits(
    rpc: &RpcClient,
    snapshot: &Snapshot,
) -> Result<(u32, u32), String> {
    const FUNCTION: &str = "get_free_unload_token_info";
    let definition = snapshot
        .metadata
        .view_function("Coinage", FUNCTION)
        .ok_or("Coinage free unload token view missing")?;
    let registry = snapshot.metadata.registry();
    let valid_output = registry.resolve(definition.output_type).is_some_and(|ty| {
        matches!(&ty.type_def, TypeDef::Tuple(tuple) if tuple.fields.len() == 2
        && tuple.fields.iter().all(|field| matches!(
            registry.resolve(field.id).map(|ty| &ty.type_def),
            Some(TypeDef::Primitive(TypeDefPrimitive::U32))
        )))
    });
    if definition.inputs != 0 || !valid_output {
        return Err("invalid Coinage free unload token view contract".into());
    }
    let arguments = (definition.id, Vec::<u8>::new()).encode();
    let response = call(
        rpc,
        "state_call",
        json!([
            "RuntimeViewFunction_execute_view_function",
            hex0x(&arguments),
            hex0x(&snapshot.at)
        ]),
    )
    .await?;
    let output =
        crate::runtime::statement_allowance::view::decode_response("Coinage", FUNCTION, response)
            .map_err(|error| error.to_string())?;
    decode_exact(&output)
}

pub(super) async fn finalized(rpc: &RpcClient) -> Result<[u8; 32], String> {
    hash_value(&call(rpc, "chain_getFinalizedHead", json!([])).await?)
}

pub(super) fn hash_value(value: &Value) -> Result<[u8; 32], String> {
    parse_hash(value.as_str().ok_or("chain hash is not a string")?)
}

pub(super) fn number(value: &Value) -> Result<u64, String> {
    value
        .as_u64()
        .or_else(|| {
            value.as_str().and_then(|s| {
                s.strip_prefix("0x")
                    .and_then(|s| u64::from_str_radix(s, 16).ok())
                    .or_else(|| s.parse().ok())
            })
        })
        .ok_or_else(|| "chain number is invalid".into())
}

pub(super) async fn block_number(rpc: &RpcClient, hash: [u8; 32]) -> Result<u64, String> {
    number(&call(rpc, "chain_getHeader", json!([hex0x(&hash)])).await?["number"])
}

pub(super) async fn metadata(rpc: &RpcClient, at: [u8; 32]) -> Result<Vec<u8>, String> {
    // V16, unlike the legacy metadata RPC, includes the active extension pipeline map.
    let result = call(
        rpc,
        "state_call",
        json!(["Metadata_metadata_at_version", "0x10000000", hex0x(&at)]),
    )
    .await?;
    let bytes = hex_bytes(&result)?;
    decode_exact::<Option<Vec<u8>>>(&bytes)?
        .ok_or_else(|| "Coinage requires runtime metadata V16".into())
}

pub(super) fn hex_bytes(value: &Value) -> Result<Vec<u8>, String> {
    hex::decode(
        value
            .as_str()
            .ok_or("chain bytes are not a string")?
            .strip_prefix("0x")
            .ok_or("chain bytes lack hex prefix")?,
    )
    .map_err(|_| "chain bytes are invalid hex".into())
}

pub(super) async fn query(
    rpc: &RpcClient,
    keys: &[Vec<u8>],
    at: [u8; 32],
) -> Result<Vec<Option<Vec<u8>>>, String> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let hex_keys: Vec<_> = keys.iter().map(|key| hex0x(key)).collect();
    let result = call(rpc, "state_queryStorageAt", json!([hex_keys, hex0x(&at)])).await?;
    decode_query(&result, keys, at)
}

pub(super) fn decode_query(
    result: &Value,
    keys: &[Vec<u8>],
    at: [u8; 32],
) -> Result<Vec<Option<Vec<u8>>>, String> {
    let blocks = result.as_array().ok_or("invalid storage snapshot")?;
    if blocks.len() != 1 || hash_value(&blocks[0]["block"])? != at {
        return Err("storage response does not match requested snapshot".into());
    }
    let mut rows = HashMap::new();
    for change in blocks[0]["changes"]
        .as_array()
        .ok_or("invalid storage changes")?
    {
        let pair = change.as_array().ok_or("invalid storage change")?;
        if pair.len() != 2 {
            return Err("invalid storage change arity".into());
        }
        let key = hex_bytes(&pair[0])?;
        let value = if pair[1].is_null() {
            None
        } else {
            Some(hex_bytes(&pair[1])?)
        };
        if !keys.contains(&key) || rows.insert(key, value).is_some() {
            return Err("unexpected or repeated storage key".into());
        }
    }
    keys.iter()
        .map(|key| {
            rows.get(key)
                .cloned()
                .ok_or_else(|| "storage response omitted requested key".into())
        })
        .collect()
}

pub(super) async fn keys(
    rpc: &RpcClient,
    prefix: &[u8],
    at: [u8; 32],
) -> Result<Vec<Vec<u8>>, String> {
    let mut result = Vec::new();
    loop {
        let start = result.last().map(|key: &Vec<u8>| hex0x(key));
        let page = call(
            rpc,
            "state_getKeysPaged",
            json!([hex0x(prefix), 256, start, hex0x(&at)]),
        )
        .await?;
        let page = page.as_array().ok_or("invalid storage key page")?;
        if page.len() > 256 {
            return Err("oversized storage key page".into());
        }
        for value in page {
            let key = hex_bytes(value)?;
            if !key.starts_with(prefix) || result.last().is_some_and(|previous| previous >= &key) {
                return Err("invalid storage key pagination".into());
            }
            result.push(key);
        }
        if page.len() < 256 {
            return Ok(result);
        }
    }
}

impl HostCoinageChain {
    pub(super) async fn broadcast(
        &self,
        rpc: &RpcClient,
        extrinsic: &[u8],
        snapshot: &Snapshot,
    ) -> Result<[u8; 32], String> {
        self.ensure_session()?;
        let head = finalized(rpc).await?;
        if block_number(rpc, head).await? >= snapshot.valid_until() {
            return Err("Coinage signing snapshot expired before broadcast".into());
        }
        self.ensure_session()?;
        // Submission is performed once. Neither a timeout nor a lost author subscription authorizes a retry.
        let submit = rpc
            .subscribe::<Value>(
                "author_submitAndWatchExtrinsic",
                rpc_params![hex0x(extrinsic)],
                "author_unwatchExtrinsic",
            )
            .fuse();
        let submit_timeout = futures_timer::Delay::new(Duration::from_secs(30)).fuse();
        futures::pin_mut!(submit, submit_timeout);
        let mut watch = futures::select! {
            result = submit => result.map_err(|_| "Coinage broadcast outcome unknown; recovery required")?,
            _ = submit_timeout => return Err("Coinage broadcast outcome unknown; recovery required".into()),
        };
        let observe = async {
            let mut included = None;
            while let Some(status) = watch.next().await {
                let status = status.map_err(|_| "Coinage submission observation interrupted")?;
                if let Some(hash) = status.get("finalized").or_else(|| status.get("inBlock")) {
                    included = Some(hash_value(hash)?);
                    break;
                }
                if ["invalid", "dropped", "usurped", "finalityTimeout"]
                    .iter()
                    .any(|key| status.get(*key).is_some())
                    || status
                        .as_str()
                        .is_some_and(|s| matches!(s, "invalid" | "dropped"))
                {
                    return Err("Coinage transaction rejected; recovery required".into());
                }
            }
            let included = included.ok_or("Coinage inclusion is unknown; recovery required")?;
            let height = block_number(rpc, included).await?;
            loop {
                let head = finalized(rpc).await?;
                let finalized_height = block_number(rpc, head).await?;
                if finalized_height >= height {
                    let canonical =
                        hash_value(&call(rpc, "chain_getBlockHash", json!([height])).await?)?;
                    if canonical != included {
                        return Err("Coinage inclusion was retracted; recovery required".into());
                    }
                    self.verify_dispatch(rpc, included, extrinsic).await?;
                    return Ok(included);
                }
                if finalized_height >= snapshot.valid_until() {
                    return Err("Coinage transaction expired; recovery required".into());
                }
                futures_timer::Delay::new(Duration::from_secs(1)).await;
            }
        }
        .fuse();
        let timeout = futures_timer::Delay::new(Duration::from_secs(180)).fuse();
        futures::pin_mut!(observe, timeout);
        futures::select! {
            result = observe => result,
            _ = timeout => Err("Coinage finality is unknown; recovery required".into()),
        }
    }

    async fn verify_dispatch(
        &self,
        rpc: &RpcClient,
        at: [u8; 32],
        submitted: &[u8],
    ) -> Result<(), String> {
        let block = call(rpc, "chain_getBlock", json!([hex0x(&at)])).await?;
        let mut index = None;
        for (i, value) in block["block"]["extrinsics"]
            .as_array()
            .ok_or("missing block extrinsics")?
            .iter()
            .enumerate()
        {
            if hex_bytes(value)? == submitted {
                if index.replace(i as u32).is_some() {
                    return Err("duplicate submitted extrinsic in block".into());
                }
            }
        }
        let index = index.ok_or("finalized block does not contain submitted extrinsic")?;
        // Events are encoded by the runtime which executed the block, i.e. the parent's state.
        let parent = hash_value(&block["block"]["header"]["parentHash"])?;
        let execution = self.snapshot(rpc, parent).await?;
        let mut key = sp_crypto_hashing::twox_128(b"System").to_vec();
        key.extend_from_slice(&sp_crypto_hashing::twox_128(b"Events"));
        let bytes = query(rpc, &[key], at)
            .await?
            .pop()
            .flatten()
            .ok_or("finalized System.Events missing")?;
        dispatch_success(&execution, block_number(rpc, at).await?, bytes, index)
    }
}

pub(super) fn dispatch_success(
    snapshot: &Snapshot,
    height: u64,
    bytes: Vec<u8>,
    index: u32,
) -> Result<(), String> {
    use subxt::config::substrate::{SpecVersionForRange, SubstrateConfig};
    let config = SubstrateConfig::builder()
        .set_metadata_for_spec_versions([(snapshot.state.spec_version, snapshot.subxt.clone())])
        .set_spec_version_for_block_ranges([SpecVersionForRange {
            block_range: 0..u64::MAX,
            spec_version: snapshot.state.spec_version,
            transaction_version: snapshot.state.transaction_version,
        }])
        .build();
    let client = subxt::OfflineClient::new_with_config(config);
    let at = client
        .at_block(height)
        .map_err(|_| "cannot prepare event decoder")?;
    let events = at.events().from_bytes(bytes);
    let mut success = false;
    for event in events.iter() {
        let event = event.map_err(|_| "cannot decode finalized events")?;
        if event.phase() != subxt::events::Phase::ApplyExtrinsic(index)
            || event.pallet_name() != "System"
        {
            continue;
        }
        match event.event_name() {
            "ExtrinsicFailed" => return Err("Coinage transaction failed on chain".into()),
            "ExtrinsicSuccess" if success => return Err("duplicate transaction outcome".into()),
            "ExtrinsicSuccess" => success = true,
            _ => (),
        }
    }
    if success {
        Ok(())
    } else {
        Err("finalized transaction has no explicit success event".into())
    }
}
