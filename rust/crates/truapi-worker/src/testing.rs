//! Drive a worker natively, build the frames a host would send it, and keep
//! its wire behaviour as a transcript a test compares byte for byte.
//!
//! A transcript is text, one line per event. `start`, `frame <hex>`,
//! `suspend` and `resume` are what the host did; the indented `send <hex>` and
//! `log <text>` lines after one are what the worker answered in that turn,
//! frames first. The same file can be replayed against a wasm build of the
//! worker in a host's sandbox, which checks the ABI as well as the bytes.

use core::fmt;

use parity_scale_codec::{DecodeAll, Encode};
use truapi::CallError;
use truapi::latest::{HostRendererActionSubscribeItem, LatestOf, RenderContext, RendererNode};
use truapi::versioned::{self, FromLatest, IntoLatest, Versioned};

use crate::calls::{Call, domain_from_latest};
use crate::{Frame, Instance, ProductWorker, wire};

/// What the host does in one turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Turn {
    /// The connection opened.
    Start,
    /// One frame from the host.
    Frame(Vec<u8>),
    /// The host paused the worker.
    Suspend,
    /// The host resumed the worker.
    Resume,
}

/// What the worker answered in one turn.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Answer {
    /// Encoded frames, in the order sent.
    pub sent: Vec<Vec<u8>>,
    /// Log lines, in the order logged.
    pub logs: Vec<String>,
}

/// Turns and the answers they drew.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Transcript {
    /// Every turn with its answer, in order.
    pub turns: Vec<(Turn, Answer)>,
}

impl<W: ProductWorker> Instance<W> {
    /// Run one turn and collect what it sent and logged.
    pub fn run(&mut self, turn: &Turn) -> Answer {
        let frames = match turn {
            Turn::Start => self.start(),
            Turn::Frame(bytes) => self.on_frame(bytes),
            Turn::Suspend => self.on_suspend(),
            Turn::Resume => self.on_resume(),
        };
        Answer {
            sent: frames.iter().map(Frame::encode).collect(),
            logs: self.take_logs(),
        }
    }
}

impl Transcript {
    /// Run `turns` against a fresh instance of `worker` and keep the answers.
    pub fn record<W: ProductWorker>(worker: W, turns: Vec<Turn>) -> Self {
        let mut instance = Instance::new(worker);
        Self {
            turns: turns
                .into_iter()
                .map(|turn| {
                    let answer = instance.run(&turn);
                    (turn, answer)
                })
                .collect(),
        }
    }

    /// Read a transcript back from its text.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut turns: Vec<(Turn, Answer)> = Vec::new();
        for (number, line) in text.lines().enumerate() {
            let line = line.trim_start();
            let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
            let turn = match word {
                "start" => Turn::Start,
                "frame" => Turn::Frame(from_hex(rest)?),
                "suspend" => Turn::Suspend,
                "resume" => Turn::Resume,
                "send" | "log" => {
                    let Some((_, answer)) = turns.last_mut() else {
                        return Err(format!("line {}: an answer before any turn", number + 1));
                    };
                    if word == "send" {
                        answer.sent.push(from_hex(rest)?);
                    } else {
                        answer.logs.push(rest.to_string());
                    }
                    continue;
                }
                other => return Err(format!("line {}: unknown entry `{other}`", number + 1)),
            };
            turns.push((turn, Answer::default()));
        }
        Ok(Self { turns })
    }

    /// The host's turns alone.
    pub fn inputs(&self) -> Vec<Turn> {
        self.turns.iter().map(|(turn, _)| turn.clone()).collect()
    }
}

impl fmt::Display for Transcript {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (turn, answer) in &self.turns {
            match turn {
                Turn::Start => writeln!(formatter, "start")?,
                Turn::Frame(bytes) => writeln!(formatter, "frame {}", to_hex(bytes))?,
                Turn::Suspend => writeln!(formatter, "suspend")?,
                Turn::Resume => writeln!(formatter, "resume")?,
            }
            for frame in &answer.sent {
                writeln!(formatter, "  send {}", to_hex(frame))?;
            }
            for line in &answer.logs {
                writeln!(formatter, "  log {line}")?;
            }
        }
        Ok(())
    }
}

/// The host's answer to the call `C` the worker sent as `request_id`.
pub fn reply<C: Call>(
    request_id: &str,
    result: Result<LatestOf<C::Response>, CallError<LatestOf<C::Error>>>,
) -> Vec<u8>
where
    C::Response: FromLatest + Encode,
    C::Error: FromLatest + Encode,
{
    let result: Result<C::Response, CallError<C::Error>> = match result {
        Ok(response) => Ok(C::Response::from_latest(response, C::Response::LATEST)),
        Err(error) => Err(domain_from_latest(error)),
    };
    Frame::new(
        request_id.to_string(),
        C::IDS,
        wire::MESSAGE_TYPE_RESPONSE,
        result.encode(),
    )
    .encode()
}

/// The host opening a render for `body` as `request_id`.
pub fn render(request_id: &str, body: RenderContext) -> Vec<u8> {
    let request = versioned::renderer::ProductRendererRenderRequest::from_latest(
        LatestOf::<versioned::renderer::ProductRendererRenderRequest> {
            context: body,
            payload: Vec::new(),
        },
        versioned::renderer::ProductRendererRenderRequest::LATEST,
    );
    Frame::new(
        request_id.to_string(),
        wire::RENDERER_RENDER,
        wire::MESSAGE_TYPE_REQUEST,
        request.encode(),
    )
    .encode()
}

/// The host closing the render it opened as `request_id`.
pub fn stop_render(request_id: &str) -> Vec<u8> {
    Frame::new(
        request_id.to_string(),
        wire::RENDERER_RENDER,
        wire::MESSAGE_TYPE_STOP,
        Vec::new(),
    )
    .encode()
}

/// A press of `action_id` inside `body`, on the action subscription the
/// worker opened as `subscription`.
pub fn action(subscription: &str, body: RenderContext, action_id: &str) -> Vec<u8> {
    let item = versioned::renderer::HostRendererActionSubscribeItem::from_latest(
        HostRendererActionSubscribeItem {
            context: body,
            action_id: action_id.to_string(),
            payload: Vec::new(),
        },
        versioned::renderer::HostRendererActionSubscribeItem::LATEST,
    );
    Frame::new(
        subscription.to_string(),
        wire::RENDERER_ACTION_SUBSCRIBE,
        wire::MESSAGE_TYPE_RESPONSE,
        item.encode(),
    )
    .encode()
}

/// The tree a sent render frame carries, if it is one.
pub fn tree(frame: &[u8]) -> Option<RendererNode> {
    let frame = Frame::decode(frame)?;
    if (frame.trait_id, frame.method_id) != wire::RENDERER_RENDER
        || frame.message_type != wire::MESSAGE_TYPE_RESPONSE
    {
        return None;
    }
    versioned::renderer::ProductRendererRenderItem::decode_all(&mut &frame.payload[..])
        .ok()
        .map(IntoLatest::into_latest)
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn from_hex(text: &str) -> Result<Vec<u8>, String> {
    if !text.len().is_multiple_of(2) {
        return Err(format!("odd-length hex `{text}`"));
    }
    (0..text.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&text[index..index + 2], 16)
                .map_err(|error| format!("hex `{text}`: {error}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // The stored transcript is what a refactor is held to, so reading one back
    // must give exactly the turns and answers that were written.
    #[test]
    fn a_transcript_reads_back_as_it_was_written() {
        let transcript = Transcript {
            turns: vec![
                (
                    Turn::Start,
                    Answer {
                        sent: vec![vec![0x0c, 0xff], vec![]],
                        logs: vec!["HandshakeOk".to_string(), "two words".to_string()],
                    },
                ),
                (Turn::Frame(vec![0x01, 0xab]), Answer::default()),
                (Turn::Suspend, Answer::default()),
                (Turn::Resume, Answer::default()),
            ],
        };
        assert_eq!(Transcript::parse(&transcript.to_string()), Ok(transcript));
    }

    #[test]
    fn a_malformed_transcript_is_refused_with_its_line() {
        assert_eq!(
            Transcript::parse("  send 00"),
            Err("line 1: an answer before any turn".to_string())
        );
        assert_eq!(
            Transcript::parse("start\nframe abc"),
            Err("odd-length hex `abc`".to_string())
        );
    }
}
