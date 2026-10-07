//! The worker contract and the state machine that drives one worker.

use parity_scale_codec::{DecodeAll, Encode};
use truapi::latest::{HostRendererActionSubscribeItem, LatestOf, RenderContext, RendererNode};
use truapi::versioned::{FromLatest, IntoLatest, Versioned};
use truapi::{CallError, WIRE_CODEC_VERSION, versioned};

use crate::calls::{Call, CallFailure, Reply, SystemHandshake, domain_into_latest};
use crate::frame::Frame;
use crate::wire;

/// A Worker product. The library owns the connection; a worker owns its
/// state and answers four questions: what to call on start, which bodies it
/// draws, how a body looks now, and what a user action does.
///
/// One value serves every surface: a chat message and a Pocket card drawn by
/// the same worker read the same fields.
pub trait ProductWorker: Sized + 'static {
    /// The connection opened. The library has sent the handshake; calls made
    /// here follow it, and the renderer action subscription opens after them.
    fn start(&mut self, _cx: &mut Ctx<Self>) {}

    /// Whether this worker draws `body`. A render the host opens for a body
    /// the worker does not draw is left unanswered.
    fn draws(&self, body: &RenderContext) -> bool;

    /// The tree for `body` now. Called when the host opens its render and
    /// again on every redraw while it stays open.
    fn render(&self, body: &RenderContext, cx: &mut Ctx<Self>) -> RendererNode;

    /// A user acted inside a body this worker drew. Ask for a redraw with
    /// [`Ctx::redraw`] when the trees change.
    fn action(&mut self, _action: &HostRendererActionSubscribeItem, _cx: &mut Ctx<Self>) {}

    /// The host paused the worker. [`Ctx::paused`] already answers `true`, and
    /// every open body is redrawn after this returns.
    fn suspend(&mut self, _cx: &mut Ctx<Self>) {}

    /// The host resumed the worker. [`Ctx::paused`] already answers `false`,
    /// and every open body is redrawn after this returns.
    fn resume(&mut self, _cx: &mut Ctx<Self>) {}
}

type Continuation<W> = Box<dyn FnOnce(&mut W, &[u8], &mut Ctx<W>)>;

/// What a worker can do from inside a hook: call the host, ask for a
/// redraw, read the paused state, and log.
pub struct Ctx<W> {
    next_id: u64,
    pending: Vec<(String, Continuation<W>)>,
    streams: Vec<(String, RenderContext)>,
    outbox: Vec<Frame>,
    logs: Vec<String>,
    paused: bool,
    redraw: bool,
}

/// What the library logs on the worker's behalf, as `Debug` lines.
#[derive(Debug)]
#[allow(
    dead_code,
    reason = "the derived Debug output is the log line, and reads every field"
)]
enum Event {
    HandshakeOk,
    CallFailed { call: &'static str, error: String },
    RenderOpened { request_id: String, body: String },
    RenderClosed { request_id: String },
    ActionReceived { action_id: String },
}

impl<W: ProductWorker> Ctx<W> {
    fn new() -> Self {
        Self {
            next_id: 0,
            pending: Vec::new(),
            streams: Vec::new(),
            outbox: Vec::new(),
            logs: Vec::new(),
            paused: false,
            redraw: false,
        }
    }

    /// Call the host method `C` with its latest request payload. `then` runs
    /// with the answer in the turn it arrives. A failure is also logged.
    pub fn call<C: Call>(
        &mut self,
        request: LatestOf<C::Request>,
        then: impl FnOnce(&mut W, Reply<C>, &mut Ctx<W>) + 'static,
    ) {
        let request_id = self.mint();
        let payload = C::Request::from_latest(request, C::Request::LATEST).encode();
        self.outbox.push(Frame::new(
            request_id.clone(),
            C::IDS,
            wire::MESSAGE_TYPE_REQUEST,
            payload,
        ));
        self.pending.push((
            request_id,
            Box::new(move |worker, payload, cx| {
                let reply = cx.settle::<C>(payload);
                then(worker, reply, cx);
            }),
        ));
    }

    /// Redraw every open body once the current hook returns.
    pub fn redraw(&mut self) {
        self.redraw = true;
    }

    /// Whether the host has paused the worker.
    pub fn paused(&self) -> bool {
        self.paused
    }

    /// Record one line of evidence; the host collects it after the turn.
    pub fn log(&mut self, line: impl core::fmt::Display) {
        self.logs.push(line.to_string());
    }

    fn event(&mut self, event: Event) {
        self.logs.push(format!("{event:?}"));
    }

    fn mint(&mut self) -> String {
        self.next_id += 1;
        format!("w:{}", self.next_id)
    }

    fn settle<C: Call>(&mut self, payload: &[u8]) -> Reply<C> {
        let decoded = Result::<C::Response, CallError<C::Error>>::decode_all(&mut &payload[..]);
        match decoded {
            Ok(Ok(response)) => Ok(response.into_latest()),
            Ok(Err(error)) => {
                self.event(Event::CallFailed {
                    call: C::NAME,
                    error: format!("{error:?}"),
                });
                Err(CallFailure::Host(domain_into_latest(error)))
            }
            Err(error) => {
                self.event(Event::CallFailed {
                    call: C::NAME,
                    error: format!("undecodable response: {error}"),
                });
                Err(CallFailure::Undecodable(error.to_string()))
            }
        }
    }
}

/// One running worker: the product's value and its connection state. The
/// exports [`export_worker!`](crate::export_worker) generates drive it turn
/// by turn; tests drive it natively the same way.
pub struct Instance<W> {
    worker: W,
    cx: Ctx<W>,
}

impl<W: ProductWorker> Instance<W> {
    /// A worker whose connection has not opened yet.
    pub fn new(worker: W) -> Self {
        Self {
            worker,
            cx: Ctx::new(),
        }
    }

    /// The worker's own state.
    pub fn worker(&self) -> &W {
        &self.worker
    }

    /// The connection opened: handshake, the worker's opening calls, then the
    /// renderer action subscription.
    pub fn start(&mut self) -> Vec<Frame> {
        self.cx.call::<SystemHandshake>(
            LatestOf::<versioned::system::HostHandshakeRequest> {
                codec_version: WIRE_CODEC_VERSION,
            },
            |_, reply, cx| {
                if reply.is_ok() {
                    cx.event(Event::HandshakeOk);
                }
            },
        );
        self.worker.start(&mut self.cx);
        let subscription = self.cx.mint();
        self.cx.outbox.push(Frame::new(
            subscription,
            wire::RENDERER_ACTION_SUBSCRIBE,
            wire::MESSAGE_TYPE_REQUEST,
            versioned::renderer::HostRendererActionSubscribeRequest::from_latest((), 1).encode(),
        ));
        self.finish_turn()
    }

    /// One frame from the host, and the frames it causes.
    pub fn on_frame(&mut self, bytes: &[u8]) -> Vec<Frame> {
        if let Some(frame) = Frame::decode(bytes) {
            match (frame.trait_id, frame.method_id) {
                wire::RENDERER_RENDER => self.on_render(frame),
                wire::RENDERER_ACTION_SUBSCRIBE => self.on_action(&frame),
                _ => self.on_reply(&frame),
            }
        }
        self.finish_turn()
    }

    /// The host paused the worker.
    pub fn on_suspend(&mut self) -> Vec<Frame> {
        self.cx.paused = true;
        self.cx.redraw = true;
        self.worker.suspend(&mut self.cx);
        self.finish_turn()
    }

    /// The host resumed the worker.
    pub fn on_resume(&mut self) -> Vec<Frame> {
        self.cx.paused = false;
        self.cx.redraw = true;
        self.worker.resume(&mut self.cx);
        self.finish_turn()
    }

    /// Lines logged since the last call, oldest first.
    pub fn take_logs(&mut self) -> Vec<String> {
        core::mem::take(&mut self.cx.logs)
    }

    fn on_reply(&mut self, frame: &Frame) {
        let position = self
            .cx
            .pending
            .iter()
            .position(|(id, _)| *id == frame.request_id);
        if let Some(position) = position {
            let (_, then) = self.cx.pending.remove(position);
            then(&mut self.worker, &frame.payload, &mut self.cx);
        }
    }

    fn on_render(&mut self, frame: Frame) {
        match frame.message_type {
            wire::MESSAGE_TYPE_REQUEST => {
                let Ok(request) = versioned::renderer::ProductRendererRenderRequest::decode_all(
                    &mut &frame.payload[..],
                ) else {
                    return;
                };
                let body = request.into_latest().context;
                if !self.worker.draws(&body) {
                    return;
                }
                self.cx.event(Event::RenderOpened {
                    request_id: frame.request_id.clone(),
                    body: body_id(&body).to_string(),
                });
                self.cx
                    .streams
                    .push((frame.request_id.clone(), body.clone()));
                self.draw(frame.request_id, &body);
            }
            wire::MESSAGE_TYPE_STOP => {
                self.cx.streams.retain(|(id, _)| *id != frame.request_id);
                self.cx.event(Event::RenderClosed {
                    request_id: frame.request_id,
                });
            }
            _ => {}
        }
    }

    fn on_action(&mut self, frame: &Frame) {
        if frame.message_type != wire::MESSAGE_TYPE_RESPONSE {
            return;
        }
        let Ok(item) = versioned::renderer::HostRendererActionSubscribeItem::decode_all(
            &mut &frame.payload[..],
        ) else {
            return;
        };
        let action = item.into_latest();
        self.cx.event(Event::ActionReceived {
            action_id: action.action_id.clone(),
        });
        self.worker.action(&action, &mut self.cx);
    }

    fn draw(&mut self, request_id: String, body: &RenderContext) {
        let tree = self.worker.render(body, &mut self.cx);
        let item = versioned::renderer::ProductRendererRenderItem::from_latest(
            tree,
            versioned::renderer::ProductRendererRenderItem::LATEST,
        );
        self.cx.outbox.push(Frame::new(
            request_id,
            wire::RENDERER_RENDER,
            wire::MESSAGE_TYPE_RESPONSE,
            item.encode(),
        ));
    }

    fn finish_turn(&mut self) -> Vec<Frame> {
        if core::mem::take(&mut self.cx.redraw) {
            for (request_id, body) in self.cx.streams.clone() {
                self.draw(request_id, &body);
            }
        }
        core::mem::take(&mut self.cx.outbox)
    }
}

/// The id a render context names its body by.
fn body_id(body: &RenderContext) -> &str {
    match body {
        RenderContext::ChatMessage { message_id, .. } => message_id,
        RenderContext::InputWidget { candidate_id } => candidate_id,
        RenderContext::PocketCard { card_id } => card_id,
    }
}
