//! Native v2 call payload codec shared by the iOS and Android v2 apps.
//!
//! `DataChannelOffer`, `DataChannelAnswer`, and `DataChannelCandidates` carry
//! these bytes inside the chat-v2 message envelope. The native apps do not put
//! raw SDP strings directly in those fields.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::{
    ChatError, Cursor, V2DataChannelPurpose, decode_compact_u32, encode_bytes, encode_compact_u32,
    encode_compact_u128, encode_string,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V2CallSdpType {
    Offer,
    Answer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V2CallTransportType {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V2CallCandidateType {
    Host,
    Srflx,
    Relay,
    Prflx,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum V2CallIpAddress {
    Ipv4([u8; 4]),
    Ipv6([u16; 8]),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2CallMinimalCandidate {
    pub foundation: String,
    pub priority: u32,
    pub transport_type: V2CallTransportType,
    pub address: V2CallIpAddress,
    pub port: u16,
    pub candidate_type: V2CallCandidateType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2CallSetup {
    pub sdp_type: V2CallSdpType,
    pub session_id: u128,
    pub session_version: u128,
    pub ice_ufrag: String,
    pub ice_pwd: String,
    pub fingerprint: Vec<u8>,
    pub candidates: Vec<V2CallMinimalCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2CallIceCandidate {
    pub sdp: String,
    pub sdp_m_line_index: u32,
    pub sdp_mid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2CallDataChannelMessage {
    pub id: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum V2CallPeerConnectionSignal {
    Offer(String),
    Answer(String),
    Candidates(Vec<V2CallIceCandidate>),
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2CallDecodedSetup {
    pub setup_sdp: String,
    pub candidates: Vec<V2CallIceCandidate>,
}

/// Use-case id used by the iOS and Android v2 apps for media renegotiation
/// messages sent through the bootstrapped WebRTC data channel.
pub const V2_CALL_RENEGOTIATION_USE_CASE_ID: &str = "webrtc_renegotiation_internal_use_case";

/// Encode a native v2 data-channel setup payload.
pub fn encode_v2_call_setup_payload(setup: &V2CallSetup) -> Result<Vec<u8>, ChatError> {
    let mut out = Vec::new();
    out.push(sdp_type_index(setup.sdp_type));
    out.extend_from_slice(&encode_compact_u128(setup.session_id));
    out.extend_from_slice(&encode_compact_u128(setup.session_version));
    encode_string(&mut out, &setup.ice_ufrag)?;
    encode_string(&mut out, &setup.ice_pwd)?;
    encode_bytes(&mut out, &setup.fingerprint)?;
    encode_v2_call_minimal_candidates(&mut out, &setup.candidates)?;
    Ok(out)
}

/// Decode a native v2 data-channel setup payload.
pub fn decode_v2_call_setup_payload(data: &[u8]) -> Result<V2CallSetup, ChatError> {
    let mut cursor = Cursor::new(data);
    let setup = V2CallSetup {
        sdp_type: decode_sdp_type(cursor.read_u8("call_setup.sdp_type")?)?,
        session_id: cursor.read_compact_u128("call_setup.session_id")?,
        session_version: cursor.read_compact_u128("call_setup.session_version")?,
        ice_ufrag: cursor.read_string("call_setup.ice_ufrag")?,
        ice_pwd: cursor.read_string("call_setup.ice_pwd")?,
        fingerprint: cursor.read_bytes("call_setup.fingerprint")?,
        candidates: decode_v2_call_minimal_candidates(&mut cursor, "call_setup.candidates")?,
    };
    cursor.finish()?;
    Ok(setup)
}

/// Parse full SDP plus WebRTC candidates and encode the native v2 setup bytes.
pub fn encode_v2_call_setup_from_sdp(
    setup_sdp: &str,
    candidates: &[V2CallIceCandidate],
) -> Result<Vec<u8>, ChatError> {
    let mut setup = parse_v2_call_setup_sdp(setup_sdp)?;
    setup.candidates = candidates
        .iter()
        .map(parse_v2_call_ice_candidate)
        .collect::<Result<Vec<_>, _>>()?;
    encode_v2_call_setup_payload(&setup)
}

/// Decode native v2 setup bytes back into the SDP shape accepted by WebRTC.
pub fn decode_v2_call_setup_to_sdp(data: &[u8]) -> Result<V2CallDecodedSetup, ChatError> {
    let setup = decode_v2_call_setup_payload(data)?;
    Ok(V2CallDecodedSetup {
        setup_sdp: reconstruct_v2_call_setup_sdp(&setup),
        candidates: setup
            .candidates
            .iter()
            .map(reconstruct_v2_call_ice_candidate)
            .collect(),
    })
}

/// Encode native v2 trickled ICE candidate payload bytes.
pub fn encode_v2_call_candidates_payload(
    candidates: &[V2CallMinimalCandidate],
) -> Result<Vec<u8>, ChatError> {
    let mut out = Vec::new();
    encode_v2_call_minimal_candidates(&mut out, candidates)?;
    Ok(out)
}

/// Decode native v2 trickled ICE candidate payload bytes.
pub fn decode_v2_call_candidates_payload(
    data: &[u8],
) -> Result<Vec<V2CallMinimalCandidate>, ChatError> {
    let mut cursor = Cursor::new(data);
    let candidates = decode_v2_call_minimal_candidates(&mut cursor, "call_candidates")?;
    cursor.finish()?;
    Ok(candidates)
}

/// Parse WebRTC candidates and encode native v2 trickled ICE candidate bytes.
pub fn encode_v2_call_candidates_from_sdp(
    candidates: &[V2CallIceCandidate],
) -> Result<Vec<u8>, ChatError> {
    let candidates = candidates
        .iter()
        .map(parse_v2_call_ice_candidate)
        .collect::<Result<Vec<_>, _>>()?;
    encode_v2_call_candidates_payload(&candidates)
}

/// Decode native v2 trickled ICE candidate bytes back into WebRTC candidates.
pub fn decode_v2_call_candidates_to_sdp(data: &[u8]) -> Result<Vec<V2CallIceCandidate>, ChatError> {
    Ok(decode_v2_call_candidates_payload(data)?
        .iter()
        .map(reconstruct_v2_call_ice_candidate)
        .collect())
}

/// Encode the v2 apps' generic WebRTC data-channel message wrapper.
pub fn encode_v2_call_data_channel_message(
    message: &V2CallDataChannelMessage,
) -> Result<Vec<u8>, ChatError> {
    let mut out = Vec::new();
    encode_string(&mut out, &message.id)?;
    encode_bytes(&mut out, &message.data)?;
    Ok(out)
}

/// Decode the v2 apps' generic WebRTC data-channel message wrapper.
pub fn decode_v2_call_data_channel_message(
    data: &[u8],
) -> Result<V2CallDataChannelMessage, ChatError> {
    let mut cursor = Cursor::new(data);
    let message = V2CallDataChannelMessage {
        id: cursor.read_string("call_data_channel_message.id")?,
        data: cursor.read_bytes("call_data_channel_message.data")?,
    };
    cursor.finish()?;
    Ok(message)
}

/// Encode the v2 apps' media renegotiation signal carried inside
/// `V2_CALL_RENEGOTIATION_USE_CASE_ID` data-channel messages.
pub fn encode_v2_call_peer_connection_signal(
    signal: &V2CallPeerConnectionSignal,
) -> Result<Vec<u8>, ChatError> {
    let mut out = Vec::new();
    match signal {
        V2CallPeerConnectionSignal::Offer(sdp) => {
            out.push(0);
            encode_string(&mut out, sdp)?;
        }
        V2CallPeerConnectionSignal::Answer(sdp) => {
            out.push(1);
            encode_string(&mut out, sdp)?;
        }
        V2CallPeerConnectionSignal::Candidates(candidates) => {
            out.push(2);
            let len = u32::try_from(candidates.len()).map_err(|_| {
                ChatError::InvalidEncoding("call peer candidate vector is too large".into())
            })?;
            out.extend_from_slice(&encode_compact_u32(len));
            for candidate in candidates {
                encode_string(&mut out, &candidate.sdp)?;
                out.extend_from_slice(&candidate.sdp_m_line_index.to_le_bytes());
                match &candidate.sdp_mid {
                    Some(sdp_mid) => {
                        out.push(1);
                        encode_string(&mut out, sdp_mid)?;
                    }
                    None => out.push(0),
                }
            }
        }
        V2CallPeerConnectionSignal::Closed => out.push(3),
    }
    Ok(out)
}

/// Decode the v2 apps' media renegotiation signal carried inside a data channel.
pub fn decode_v2_call_peer_connection_signal(
    data: &[u8],
) -> Result<V2CallPeerConnectionSignal, ChatError> {
    let mut cursor = Cursor::new(data);
    let signal = match cursor.read_u8("call_peer_connection_signal.index")? {
        0 => V2CallPeerConnectionSignal::Offer(
            cursor.read_string("call_peer_connection_signal.offer")?,
        ),
        1 => V2CallPeerConnectionSignal::Answer(
            cursor.read_string("call_peer_connection_signal.answer")?,
        ),
        2 => {
            let (len, consumed) =
                decode_compact_u32(&cursor.data[cursor.offset..]).map_err(|e| {
                    ChatError::InvalidEncoding(format!(
                        "call_peer_connection_signal.candidates: {e}"
                    ))
                })?;
            cursor.offset += consumed;
            let len = len as usize;
            if len > cursor.data.len().saturating_sub(cursor.offset) / 6 {
                return Err(ChatError::InvalidEncoding(
                    "call_peer_connection_signal.candidates: item count exceeds remaining input"
                        .into(),
                ));
            }
            let mut candidates = Vec::with_capacity(len);
            for index in 0..len {
                let field = format!("call_peer_connection_signal.candidates[{index}]");
                candidates.push(V2CallIceCandidate {
                    sdp: cursor.read_string(&format!("{field}.sdp"))?,
                    sdp_m_line_index: cursor.read_u32(&format!("{field}.sdp_m_line_index"))?,
                    sdp_mid: match cursor.read_u8(&format!("{field}.sdp_mid"))? {
                        0 => None,
                        1 => Some(cursor.read_string(&format!("{field}.sdp_mid.value"))?),
                        value => {
                            return Err(ChatError::InvalidEncoding(format!(
                                "{field}.sdp_mid has invalid option index {value}"
                            )));
                        }
                    },
                });
            }
            V2CallPeerConnectionSignal::Candidates(candidates)
        }
        3 => V2CallPeerConnectionSignal::Closed,
        value => {
            return Err(ChatError::InvalidEncoding(format!(
                "unsupported call peer connection signal index {value}"
            )));
        }
    };
    cursor.finish()?;
    Ok(signal)
}

/// Encode a media renegotiation signal in the data-channel envelope expected by
/// iOS v2 and Android v2.
pub fn encode_v2_call_renegotiation_message(
    signal: &V2CallPeerConnectionSignal,
) -> Result<Vec<u8>, ChatError> {
    encode_v2_call_data_channel_message(&V2CallDataChannelMessage {
        id: V2_CALL_RENEGOTIATION_USE_CASE_ID.into(),
        data: encode_v2_call_peer_connection_signal(signal)?,
    })
}

/// Decode a media renegotiation signal from the data-channel envelope expected
/// by iOS v2 and Android v2.
pub fn decode_v2_call_renegotiation_message(
    data: &[u8],
) -> Result<V2CallPeerConnectionSignal, ChatError> {
    let message = decode_v2_call_data_channel_message(data)?;
    if message.id != V2_CALL_RENEGOTIATION_USE_CASE_ID {
        return Err(ChatError::InvalidEncoding(format!(
            "unexpected call data-channel message id {}",
            message.id
        )));
    }
    decode_v2_call_peer_connection_signal(&message.data)
}

pub fn v2_call_purpose_from_video(with_video: bool) -> V2DataChannelPurpose {
    if with_video {
        V2DataChannelPurpose::Video
    } else {
        V2DataChannelPurpose::Audio
    }
}

fn parse_v2_call_setup_sdp(setup_sdp: &str) -> Result<V2CallSetup, ChatError> {
    let mut ice_ufrag = None;
    let mut ice_pwd = None;
    let mut fingerprint = None;
    let mut sdp_type = V2CallSdpType::Offer;
    let mut session_id = None;
    let mut session_version = None;

    for line in setup_sdp.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("a=ice-ufrag:") {
            ice_ufrag = Some(value.to_string());
        } else if let Some(value) = trimmed.strip_prefix("a=ice-pwd:") {
            ice_pwd = Some(value.to_string());
        } else if let Some(value) = trimmed.strip_prefix("a=fingerprint:") {
            fingerprint = Some(parse_fingerprint(value)?);
        } else if let Some(value) = trimmed.strip_prefix("a=setup:") {
            sdp_type = if value == "actpass" {
                V2CallSdpType::Offer
            } else {
                V2CallSdpType::Answer
            };
        } else if let Some(value) = trimmed.strip_prefix("o=") {
            let parts = value.split_whitespace().collect::<Vec<_>>();
            if parts.len() < 3 {
                return Err(ChatError::InvalidEncoding(
                    "call setup SDP has invalid o= session line".into(),
                ));
            }
            session_id = Some(parts[1].parse::<u128>().map_err(|_| {
                ChatError::InvalidEncoding("call setup SDP has invalid session id".into())
            })?);
            session_version = Some(parts[2].parse::<u128>().map_err(|_| {
                ChatError::InvalidEncoding("call setup SDP has invalid session version".into())
            })?);
        }
    }

    Ok(V2CallSetup {
        sdp_type,
        session_id: session_id.ok_or_else(|| {
            ChatError::InvalidEncoding("call setup SDP missing o= session line".into())
        })?,
        session_version: session_version.ok_or_else(|| {
            ChatError::InvalidEncoding("call setup SDP missing o= session line".into())
        })?,
        ice_ufrag: ice_ufrag.ok_or_else(|| {
            ChatError::InvalidEncoding("call setup SDP missing a=ice-ufrag".into())
        })?,
        ice_pwd: ice_pwd
            .ok_or_else(|| ChatError::InvalidEncoding("call setup SDP missing a=ice-pwd".into()))?,
        fingerprint: fingerprint.ok_or_else(|| {
            ChatError::InvalidEncoding("call setup SDP missing a=fingerprint".into())
        })?,
        candidates: Vec::new(),
    })
}

fn parse_v2_call_ice_candidate(
    candidate: &V2CallIceCandidate,
) -> Result<V2CallMinimalCandidate, ChatError> {
    if candidate.sdp_m_line_index != 0 {
        return Err(ChatError::InvalidEncoding(format!(
            "call ICE candidate has unsupported sdpMLineIndex {}",
            candidate.sdp_m_line_index
        )));
    }
    if !matches!(candidate.sdp_mid.as_deref(), None | Some("0")) {
        return Err(ChatError::InvalidEncoding(format!(
            "call ICE candidate has unsupported sdpMid {:?}",
            candidate.sdp_mid
        )));
    }

    let trimmed = candidate.sdp.trim();
    let content = trimmed.strip_prefix("candidate:").ok_or_else(|| {
        ChatError::InvalidEncoding("call ICE candidate missing candidate: prefix".into())
    })?;
    let parts = content.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 8 || parts[6] != "typ" {
        return Err(ChatError::InvalidEncoding(
            "call ICE candidate has invalid format".into(),
        ));
    }

    let component_id = parts[1].parse::<u8>().map_err(|_| {
        ChatError::InvalidEncoding("call ICE candidate has invalid component id".into())
    })?;
    if component_id != 1 {
        return Err(ChatError::InvalidEncoding(format!(
            "call ICE candidate has unsupported component id {component_id}"
        )));
    }

    let priority = parts[3].parse::<u32>().map_err(|_| {
        ChatError::InvalidEncoding("call ICE candidate has invalid priority".into())
    })?;
    validate_call_priority(priority)?;

    Ok(V2CallMinimalCandidate {
        foundation: parts[0].to_string(),
        priority,
        transport_type: parse_transport_type(parts[2])?,
        address: parse_ip_address(parts[4])?,
        port: parts[5].parse::<u16>().map_err(|_| {
            ChatError::InvalidEncoding("call ICE candidate has invalid port".into())
        })?,
        candidate_type: parse_candidate_type(parts[7])?,
    })
}

/// ICE candidate priority must fit in a signed 32-bit integer to round-trip
/// through the Android v2 `Int` representation. Enforced on every path that
/// produces or consumes the binary candidate form so the SDP parser, the
/// binary encoder, and the binary decoder agree on the legal range.
fn validate_call_priority(priority: u32) -> Result<(), ChatError> {
    if priority > i32::MAX as u32 {
        return Err(ChatError::InvalidEncoding(format!(
            "call ICE candidate priority {priority} exceeds Android v2 Int range"
        )));
    }
    Ok(())
}

fn reconstruct_v2_call_setup_sdp(setup: &V2CallSetup) -> String {
    let setup_value = match setup.sdp_type {
        V2CallSdpType::Offer => "actpass",
        V2CallSdpType::Answer => "active",
    };
    format!(
        "v=0\n\
         o=- {} {} IN IP4 0.0.0.0\n\
         s=-\n\
         t=0 0\n\
         m=application 9 UDP/DTLS/SCTP webrtc-datachannel\n\
         c=IN IP4 0.0.0.0\n\
         a=ice-ufrag:{}\n\
         a=ice-pwd:{}\n\
         a=fingerprint:{}\n\
         a=setup:{}\n\
         a=mid:0\n\
         a=sctp-port:5000\n",
        setup.session_id,
        setup.session_version,
        setup.ice_ufrag,
        setup.ice_pwd,
        format_fingerprint(&setup.fingerprint),
        setup_value
    )
}

fn reconstruct_v2_call_ice_candidate(candidate: &V2CallMinimalCandidate) -> V2CallIceCandidate {
    V2CallIceCandidate {
        sdp: format!(
            "candidate:{} 1 {} {} {} {} typ {}",
            candidate.foundation,
            transport_type_string(candidate.transport_type),
            candidate.priority,
            ip_address_string(&candidate.address),
            candidate.port,
            candidate_type_string(candidate.candidate_type)
        ),
        sdp_m_line_index: 0,
        sdp_mid: Some("0".into()),
    }
}

fn encode_v2_call_minimal_candidates(
    out: &mut Vec<u8>,
    candidates: &[V2CallMinimalCandidate],
) -> Result<(), ChatError> {
    let len = u32::try_from(candidates.len())
        .map_err(|_| ChatError::InvalidEncoding("call ICE candidate vector is too large".into()))?;
    out.extend_from_slice(&encode_compact_u32(len));
    for candidate in candidates {
        validate_call_priority(candidate.priority)?;
        encode_string(out, &candidate.foundation)?;
        out.extend_from_slice(&candidate.priority.to_le_bytes());
        out.push(transport_type_index(candidate.transport_type));
        encode_ip_address(out, &candidate.address);
        out.extend_from_slice(&candidate.port.to_le_bytes());
        out.push(candidate_type_index(candidate.candidate_type));
    }
    Ok(())
}

fn decode_v2_call_minimal_candidates(
    cursor: &mut Cursor<'_>,
    field: &str,
) -> Result<Vec<V2CallMinimalCandidate>, ChatError> {
    let (len, consumed) = decode_compact_u32(&cursor.data[cursor.offset..])
        .map_err(|e| ChatError::InvalidEncoding(format!("{field}: {e}")))?;
    cursor.offset += consumed;

    // Cap the pre-allocation to the bytes that actually remain (each candidate
    // needs at least one byte) so a malformed length prefix can't trigger a
    // multi-gigabyte reservation and abort the process.
    let remaining = cursor.data.len().saturating_sub(cursor.offset);
    let mut out = Vec::with_capacity((len as usize).min(remaining));
    for index in 0..len {
        let item_field = format!("{field}[{index}]");
        let foundation = cursor.read_string(&format!("{item_field}.foundation"))?;
        let priority = cursor.read_u32(&format!("{item_field}.priority"))?;
        validate_call_priority(priority)?;
        let transport_type =
            decode_transport_type(cursor.read_u8(&format!("{item_field}.transport_type"))?)?;
        let address = decode_ip_address(cursor, &format!("{item_field}.address"))?;
        let port = cursor.read_u16(&format!("{item_field}.port"))?;
        let candidate_type =
            decode_candidate_type(cursor.read_u8(&format!("{item_field}.candidate_type"))?)?;
        out.push(V2CallMinimalCandidate {
            foundation,
            priority,
            transport_type,
            address,
            port,
            candidate_type,
        });
    }
    Ok(out)
}

fn encode_ip_address(out: &mut Vec<u8>, address: &V2CallIpAddress) {
    match address {
        V2CallIpAddress::Ipv4(bytes) => {
            out.push(0);
            out.extend_from_slice(bytes);
        }
        V2CallIpAddress::Ipv6(segments) => {
            out.push(1);
            for segment in segments {
                out.extend_from_slice(&segment.to_le_bytes());
            }
        }
    }
}

fn decode_ip_address(cursor: &mut Cursor<'_>, field: &str) -> Result<V2CallIpAddress, ChatError> {
    match cursor.read_u8(field)? {
        0 => Ok(V2CallIpAddress::Ipv4(
            cursor
                .read_exact(4, field)?
                .try_into()
                .map_err(|_| ChatError::InvalidEncoding(format!("{field}: invalid IPv4")))?,
        )),
        1 => {
            let mut segments = [0_u16; 8];
            for segment in &mut segments {
                *segment = cursor.read_u16(field)?;
            }
            Ok(V2CallIpAddress::Ipv6(segments))
        }
        value => Err(ChatError::InvalidEncoding(format!(
            "{field}: unsupported IP address type {value}"
        ))),
    }
}

fn parse_fingerprint(value: &str) -> Result<Vec<u8>, ChatError> {
    // The wire format carries only the raw digest bytes with no algorithm tag,
    // and reconstruction always emits `sha-256`. Reject any other algorithm at
    // parse time rather than silently relabelling a non-sha-256 fingerprint.
    let trimmed = value.trim();
    let hex = match trimmed.split_once(' ') {
        Some((algorithm, rest)) => {
            if !algorithm.eq_ignore_ascii_case("sha-256") {
                return Err(ChatError::InvalidEncoding(format!(
                    "call SDP fingerprint uses unsupported algorithm {algorithm}; only sha-256 is supported"
                )));
            }
            rest
        }
        None => trimmed,
    }
    .replace(':', "");
    // `is_multiple_of` and the slicing below are byte-oriented, so a non-ASCII
    // value could otherwise slice across a UTF-8 char boundary and panic.
    if !hex.is_ascii() {
        return Err(ChatError::InvalidEncoding(
            "call SDP fingerprint is not hex".into(),
        ));
    }
    if !hex.len().is_multiple_of(2) {
        return Err(ChatError::InvalidEncoding(
            "call SDP fingerprint hex has odd length".into(),
        ));
    }
    (0..hex.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&hex[index..index + 2], 16)
                .map_err(|_| ChatError::InvalidEncoding("call SDP fingerprint is not hex".into()))
        })
        .collect()
}

fn format_fingerprint(data: &[u8]) -> String {
    let mut out = String::from("sha-256 ");
    for (index, byte) in data.iter().enumerate() {
        if index > 0 {
            out.push(':');
        }
        out.push_str(&format!("{byte:02X}"));
    }
    out
}

fn parse_ip_address(value: &str) -> Result<V2CallIpAddress, ChatError> {
    match value.parse::<IpAddr>().map_err(|_| {
        ChatError::InvalidEncoding(format!("call ICE candidate has invalid IP address {value}"))
    })? {
        IpAddr::V4(address) => Ok(V2CallIpAddress::Ipv4(address.octets())),
        IpAddr::V6(address) => Ok(V2CallIpAddress::Ipv6(address.segments())),
    }
}

fn ip_address_string(address: &V2CallIpAddress) -> String {
    match address {
        V2CallIpAddress::Ipv4(bytes) => {
            Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]).to_string()
        }
        V2CallIpAddress::Ipv6(segments) => Ipv6Addr::new(
            segments[0],
            segments[1],
            segments[2],
            segments[3],
            segments[4],
            segments[5],
            segments[6],
            segments[7],
        )
        .to_string(),
    }
}

fn sdp_type_index(value: V2CallSdpType) -> u8 {
    match value {
        V2CallSdpType::Offer => 0,
        V2CallSdpType::Answer => 1,
    }
}

fn decode_sdp_type(value: u8) -> Result<V2CallSdpType, ChatError> {
    match value {
        0 => Ok(V2CallSdpType::Offer),
        1 => Ok(V2CallSdpType::Answer),
        value => Err(ChatError::InvalidEncoding(format!(
            "unsupported call SDP type {value}"
        ))),
    }
}

fn parse_transport_type(value: &str) -> Result<V2CallTransportType, ChatError> {
    match value.to_ascii_lowercase().as_str() {
        "tcp" => Ok(V2CallTransportType::Tcp),
        "udp" => Ok(V2CallTransportType::Udp),
        _ => Err(ChatError::InvalidEncoding(format!(
            "unsupported call ICE transport {value}"
        ))),
    }
}

fn transport_type_index(value: V2CallTransportType) -> u8 {
    match value {
        V2CallTransportType::Tcp => 0,
        V2CallTransportType::Udp => 1,
    }
}

fn decode_transport_type(value: u8) -> Result<V2CallTransportType, ChatError> {
    match value {
        0 => Ok(V2CallTransportType::Tcp),
        1 => Ok(V2CallTransportType::Udp),
        value => Err(ChatError::InvalidEncoding(format!(
            "unsupported call ICE transport type {value}"
        ))),
    }
}

fn transport_type_string(value: V2CallTransportType) -> &'static str {
    match value {
        V2CallTransportType::Tcp => "TCP",
        V2CallTransportType::Udp => "UDP",
    }
}

fn parse_candidate_type(value: &str) -> Result<V2CallCandidateType, ChatError> {
    match value.to_ascii_lowercase().as_str() {
        "host" => Ok(V2CallCandidateType::Host),
        "srflx" => Ok(V2CallCandidateType::Srflx),
        "relay" => Ok(V2CallCandidateType::Relay),
        "prflx" => Ok(V2CallCandidateType::Prflx),
        _ => Err(ChatError::InvalidEncoding(format!(
            "unsupported call ICE candidate type {value}"
        ))),
    }
}

fn candidate_type_index(value: V2CallCandidateType) -> u8 {
    match value {
        V2CallCandidateType::Host => 0,
        V2CallCandidateType::Srflx => 1,
        V2CallCandidateType::Relay => 2,
        V2CallCandidateType::Prflx => 3,
    }
}

fn candidate_type_string(value: V2CallCandidateType) -> &'static str {
    match value {
        V2CallCandidateType::Host => "host",
        V2CallCandidateType::Srflx => "srflx",
        V2CallCandidateType::Relay => "relay",
        V2CallCandidateType::Prflx => "prflx",
    }
}

fn decode_candidate_type(value: u8) -> Result<V2CallCandidateType, ChatError> {
    match value {
        0 => Ok(V2CallCandidateType::Host),
        1 => Ok(V2CallCandidateType::Srflx),
        2 => Ok(V2CallCandidateType::Relay),
        3 => Ok(V2CallCandidateType::Prflx),
        value => Err(ChatError::InvalidEncoding(format!(
            "unsupported call ICE candidate type {value}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OFFER_SDP: &str = "v=0\n\
        o=- 12345 67890 IN IP4 0.0.0.0\n\
        s=-\nt=0 0\n\
        a=ice-ufrag:ufrag123\n\
        a=ice-pwd:pwd123\n\
        a=fingerprint:sha-256 A1:B2:C3:D4\n\
        a=setup:actpass\n";

    fn ice(sdp: &str) -> V2CallIceCandidate {
        V2CallIceCandidate {
            sdp: sdp.into(),
            sdp_m_line_index: 0,
            sdp_mid: Some("0".into()),
        }
    }

    // Fix 1: a malformed length prefix must not trigger a huge allocation; it
    // should fail cleanly as truncated input.
    #[test]
    fn decode_candidates_rejects_oversized_length_without_oom() {
        let mut data = encode_compact_u32(u32::MAX).to_vec();
        data.extend_from_slice(&[0x04, 0x31]); // one tiny partial candidate, then EOF
        let err = decode_v2_call_candidates_payload(&data).unwrap_err();
        assert!(matches!(err, ChatError::InvalidEncoding(_)));
    }

    #[test]
    fn decode_peer_signal_rejects_oversized_length_without_oom() {
        let data = [2, 3, 0xff, 0xff, 0xff, 0xff];
        let err = decode_v2_call_peer_connection_signal(&data).unwrap_err();
        assert!(matches!(err, ChatError::InvalidEncoding(_)));
    }

    // Fix 2: a non-ASCII fingerprint must error, not panic on a char boundary.
    #[test]
    fn parse_fingerprint_rejects_non_ascii_without_panic() {
        let err = parse_fingerprint("sha-256 A£B").unwrap_err();
        assert!(matches!(err, ChatError::InvalidEncoding(_)));
    }

    // Fix 3: the wire format is sha-256 only; other algorithms must be rejected
    // rather than silently relabelled on reconstruction.
    #[test]
    fn parse_fingerprint_rejects_non_sha256_algorithm() {
        let err = parse_fingerprint("sha-1 A1:B2:C3:D4").unwrap_err();
        match err {
            ChatError::InvalidEncoding(msg) => assert!(msg.contains("unsupported algorithm")),
            other => panic!("unexpected error: {other:?}"),
        }
        // bare hex (no algorithm token) is still accepted as sha-256.
        assert_eq!(
            parse_fingerprint("A1:B2:C3:D4").unwrap(),
            vec![0xA1, 0xB2, 0xC3, 0xD4]
        );
    }

    // Fix 4: priority > i32::MAX is rejected on both the binary encode path and
    // the binary decode path, matching the SDP parser — so the codec can never
    // emit a value it would refuse to read back.
    #[test]
    fn priority_bound_enforced_on_encode_and_decode() {
        let candidate = V2CallMinimalCandidate {
            foundation: "1".into(),
            priority: i32::MAX as u32 + 1,
            transport_type: V2CallTransportType::Udp,
            address: V2CallIpAddress::Ipv4([192, 168, 1, 1]),
            port: 1234,
            candidate_type: V2CallCandidateType::Host,
        };
        assert!(encode_v2_call_candidates_payload(std::slice::from_ref(&candidate)).is_err());

        // Hand-assemble a one-candidate payload with an out-of-range priority.
        let mut data = encode_compact_u32(1).to_vec();
        data.extend_from_slice(&[0x04, 0x31]); // foundation "1"
        data.extend_from_slice(&u32::MAX.to_le_bytes()); // priority
        data.push(transport_type_index(V2CallTransportType::Udp));
        encode_ip_address(&mut data, &V2CallIpAddress::Ipv4([192, 168, 1, 1]));
        data.extend_from_slice(&1234_u16.to_le_bytes());
        data.push(candidate_type_index(V2CallCandidateType::Host));
        assert!(decode_v2_call_candidates_payload(&data).is_err());
    }

    // Fix 5: a non-canonical (zero-padded big-mode) compact session id must be
    // rejected so the wire format stays a 1:1 mapping.
    #[test]
    fn decode_setup_rejects_non_canonical_compact_session_id() {
        // sdp_type=offer, then session_id encoded in big mode as value 5 with a
        // trailing zero byte — a value that canonically fits in single-byte mode.
        let data = [0x00, 0x03, 0x05, 0x00, 0x00, 0x00];
        assert!(decode_v2_call_setup_payload(&data).is_err());
    }

    // The candidate batch with an IPv6 (relay) entry must round-trip, covering
    // the previously untested 16-byte address path.
    #[test]
    fn ipv6_candidate_round_trips() {
        let bytes = encode_v2_call_candidates_from_sdp(&[ice(
            "candidate:relay1 1 udp 123456 2001:db8::1 9999 typ relay",
        )])
        .unwrap();
        let decoded = decode_v2_call_candidates_payload(&bytes).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(
            decoded[0].address,
            V2CallIpAddress::Ipv6([0x2001, 0x0db8, 0, 0, 0, 0, 0, 1])
        );
        assert_eq!(decoded[0].candidate_type, V2CallCandidateType::Relay);
    }

    // A well-formed offer SDP still parses end-to-end after the fixes.
    #[test]
    fn offer_sdp_still_encodes() {
        let bytes = encode_v2_call_setup_from_sdp(
            OFFER_SDP,
            std::slice::from_ref(&ice(
                "candidate:1 1 udp 2122260223 192.168.1.1 1234 typ host",
            )),
        )
        .unwrap();
        let decoded = decode_v2_call_setup_payload(&bytes).unwrap();
        assert_eq!(decoded.sdp_type, V2CallSdpType::Offer);
        assert_eq!(decoded.session_id, 12_345);
        assert_eq!(decoded.candidates.len(), 1);
    }
}
