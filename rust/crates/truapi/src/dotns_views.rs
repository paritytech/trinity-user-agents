//! dotNS protocol-registry view helpers shared by the gateway and the
//! runtime's manifest readers.

use crate::host_logic::dotns_gateway::{
    DotnsTransport, DotnsViewError, call_bytes32, call_no_args, decode_address, decode_bool,
    decode_string, namehash_under, registry_key,
};

/// The TLD of networks whose `DotnsProtocolRegistry` has no `tld()` view;
/// previewnet is one.
pub const TLD_WITHOUT_VIEW: &str = ".dot";

/// Appends a dynamic `string` tail: its length, then its bytes padded to a
/// whole number of words. The caller has already written the head offset.
fn append_dynamic_string(data: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    let mut len = [0u8; 32];
    len[24..].copy_from_slice(&(bytes.len() as u64).to_be_bytes());
    data.extend_from_slice(&len);
    data.extend_from_slice(bytes);
    data.extend(std::iter::repeat_n(
        0u8,
        bytes.len().div_ceil(32) * 32 - bytes.len(),
    ));
}

/// Head word holding the byte offset a dynamic argument's tail starts at,
/// counted from the end of the selector. `head_words` is how many words the
/// head occupies.
fn dynamic_offset(head_words: usize) -> [u8; 32] {
    let mut word = [0u8; 32];
    word[24..].copy_from_slice(&((head_words * 32) as u64).to_be_bytes());
    word
}

/// Calldata for a view function taking one `string` argument.
pub fn call_string(signature: &str, value: &str) -> Vec<u8> {
    let mut data = call_no_args(signature);
    data.extend_from_slice(&dynamic_offset(1));
    append_dynamic_string(&mut data, value);
    data
}

/// Calldata for a view function taking a `bytes32` and a `string`, such as
/// `text(bytes32 node, string key)`. The string is dynamic, so the head holds
/// its offset and the tail follows.
pub fn call_bytes32_string(signature: &str, word: &[u8; 32], value: &str) -> Vec<u8> {
    let mut data = call_no_args(signature);
    data.extend_from_slice(word);
    data.extend_from_slice(&dynamic_offset(2));
    append_dynamic_string(&mut data, value);
    data
}

/// One component address out of the protocol registry's address book, so a
/// rotated implementation is picked up without a change here.
pub async fn protocol_component<T: DotnsTransport + ?Sized>(
    transport: &mut T,
    protocol_registry: &[u8; 20],
    name: &str,
) -> Result<[u8; 20], String> {
    let output = transport
        .view(
            protocol_registry,
            call_bytes32("get(bytes32)", &registry_key(name)),
        )
        .await
        .map_err(|err| format!("ProtocolRegistry.get({name}): {err}"))?;
    decode_address(&output).map_err(|err| format!("ProtocolRegistry.get({name}): {err}"))
}

/// The network TLD with its leading dot (`.paseo`), read from
/// `ProtocolRegistry.tld()`. A registry without that view reverts; the fall
/// back to [`TLD_WITHOUT_VIEW`] is then verified rather than guessed —
/// `DotnsRegistry.recordExists(namehash("dot"))` must hold the TLD's own
/// record, or the resolution errors. Any other failure is an error: a wrong
/// TLD would drop every label carrying the real one.
pub async fn network_tld<T: DotnsTransport + ?Sized>(
    transport: &mut T,
    registry: &[u8; 20],
) -> Result<String, String> {
    match transport.view(registry, call_no_args("tld()")).await {
        Ok(output) => {
            return decode_string(&output).map_err(|err| format!("ProtocolRegistry.tld(): {err}"));
        }
        Err(DotnsViewError::Reverted(_)) => {}
        Err(DotnsViewError::Failed(reason)) => {
            return Err(format!("ProtocolRegistry.tld(): {reason}"));
        }
    }
    let dotns_registry_output = transport
        .view(
            registry,
            call_bytes32("get(bytes32)", &registry_key("registry")),
        )
        .await
        .map_err(|err| format!("ProtocolRegistry.get(registry): {err}"))?;
    let dotns_registry = decode_address(&dotns_registry_output)
        .map_err(|err| format!("ProtocolRegistry.get(registry): {err}"))?;
    let exists_output = transport
        .view(
            &dotns_registry,
            call_bytes32("recordExists(bytes32)", &tld_node(TLD_WITHOUT_VIEW)),
        )
        .await
        .map_err(|err| format!("DotnsRegistry.recordExists: {err}"))?;
    if decode_bool(&exists_output).map_err(|err| format!("DotnsRegistry.recordExists: {err}"))? {
        Ok(TLD_WITHOUT_VIEW.to_string())
    } else {
        Err(format!(
            "ProtocolRegistry has no tld() view and the registry holds no record for \
             {TLD_WITHOUT_VIEW:?}; the network TLD cannot be determined"
        ))
    }
}

/// The node of the network TLD: `namehash(tld)` for a single-label TLD.
pub fn tld_node(tld: &str) -> [u8; 32] {
    namehash_under(&[0u8; 32], tld.trim_start_matches('.'))
}

/// Current owner of a bare name in either deployed dotNS representation.
/// Earlier controllers minted dotted lite names as atomic second-level tokens;
/// newer controllers register them beneath the numeric suffix. Conflicting
/// owners are an error, never permission to select whichever matches a caller.
/// Shape and PoP provenance are checked by the identity caller.
pub async fn label_owner<T: DotnsTransport + ?Sized>(
    transport: &mut T,
    registry: &[u8; 20],
    tld: &[u8; 32],
    label: &str,
) -> Result<Option<[u8; 20]>, String> {
    let atomic = node_owner(transport, registry, &namehash_under(tld, label)).await?;
    let Some((stem, suffix)) = label.split_once('.') else {
        return Ok(atomic);
    };
    let nested_node = namehash_under(&namehash_under(tld, suffix), stem);
    let nested = node_owner(transport, registry, &nested_node).await?;
    match (atomic, nested) {
        (Some(a), Some(b)) if a != b => Err("ambiguous dotNS lite name owner".into()),
        (Some(owner), _) | (_, Some(owner)) => Ok(Some(owner)),
        (None, None) => Ok(None),
    }
}

async fn node_owner<T: DotnsTransport + ?Sized>(
    transport: &mut T,
    registry: &[u8; 20],
    node: &[u8; 32],
) -> Result<Option<[u8; 20]>, String> {
    let exists = transport
        .view(registry, call_bytes32("recordExists(bytes32)", node))
        .await?;
    if exists.len() != 32 {
        return Err("dotNS recordExists returned a non-word".into());
    }
    if !decode_bool(&exists).map_err(|error| error.to_string())? {
        return Ok(None);
    }
    let output = transport
        .view(registry, call_bytes32("owner(bytes32)", node))
        .await?;
    if output.len() != 32 {
        return Err("dotNS address is not one ABI word".into());
    }
    let owner = decode_address(&output).map_err(|error| error.to_string())?;
    if owner == [0; 20] {
        return Err("existing dotNS record has a zero owner".into());
    }
    Ok(Some(owner))
}
