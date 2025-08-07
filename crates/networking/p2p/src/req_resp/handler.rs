// These deprecations are reverted in the next version of libp2p
#![allow(deprecated)]

use std::{
    collections::{HashMap, VecDeque, hash_map::Entry},
    pin::Pin,
    task::{Context, Poll},
};

use delay_map::HashSetDelay;
use futures::{FutureExt, Sink, SinkExt, StreamExt};
use libp2p::{
    Stream,
    swarm::{
        ConnectionHandler, ConnectionHandlerEvent, StreamUpgradeError, SubstreamProtocol,
        handler::{
            ConnectionEvent, DialUpgradeError, FullyNegotiatedInbound, FullyNegotiatedOutbound,
        },
    },
};
use tracing::{error, trace};

use super::{
    configurations::REQUEST_TIMEOUT,
    error::ReqRespError,
    inbound_protocol::{InboundFramed, InboundOutput, InboundReqRespProtocol, ResponseCode},
    messages::{RequestMessage, ResponseMessage},
    outbound_protocol::{OutboundFramed, OutboundReqRespProtocol},
};
use crate::req_resp::ConnectionRequest;

#[derive(Debug)]
pub enum ReqRespMessageReceived {
    Request {
        stream_id: u64,
        message: Box<RequestMessage>,
    },
    Response {
        request_id: u64,
        message: Box<ResponseMessage>,
    },
    EndOfStream {
        request_id: u64,
    },
}

#[derive(Debug)]
pub enum RespMessage {
    Response(Box<ResponseMessage>),
    Error(ReqRespError),
    EndOfStream,
}

impl RespMessage {
    pub fn as_response_code(&self) -> Option<ResponseCode> {
        match self {
            RespMessage::Response(_) => Some(ResponseCode::Success),
            RespMessage::Error(req_resp_error) => match req_resp_error {
                ReqRespError::RawError(_)
                | ReqRespError::IncompleteStream
                | ReqRespError::Anyhow(_)
                | ReqRespError::IoError(_) => Some(ResponseCode::ServerError),
                ReqRespError::InvalidData(_) => Some(ResponseCode::InvalidRequest),
                ReqRespError::Disconnected
                | ReqRespError::StreamTimedOut
                | ReqRespError::TokioTimedOut(_) => Some(ResponseCode::ResourceUnavailable),
            },
            RespMessage::EndOfStream => None,
        }
    }
}

#[derive(Debug)]
pub enum ReqRespMessageError {
    Inbound { stream_id: u64, err: ReqRespError },
    Outbound { request_id: u64, err: ReqRespError },
}

#[derive(Debug)]
pub enum HandlerEvent {
    Ok(Box<ReqRespMessageReceived>),
    Err(ReqRespMessageError),
    Close,
}

type BusyInboundStream =
    Pin<Box<dyn Future<Output = Result<Option<InboundFramed<Stream>>, ReqRespError>> + Send>>;

enum InboundStreamState {
    Idle(InboundFramed<Stream>),
    Busy(BusyInboundStream),
}

struct InboundStream {
    state: Option<InboundStreamState>,
    response_queue: VecDeque<RespMessage>,
}

enum OutboundStreamState {
    PendingResponse {
        stream: Box<OutboundFramed<Stream>>,
        message: RequestMessage,
    },
    Closing(Box<OutboundFramed<Stream>>),
}

struct OutboundStream {
    state: Option<OutboundStreamState>,
    request_id: u64,
}

#[derive(Debug)]
pub struct OutboundOpenInfo {
    pub request_id: u64,
    pub message: RequestMessage,
}

#[derive(Debug)]
enum ConnectionState {
    Live,
    ShuttingDown,
    Closed,
}

pub struct ReqRespConnectionHandler {
    listen_protocol: SubstreamProtocol<InboundReqRespProtocol, ()>,
    behaviour_events: Vec<HandlerEvent>,
    inbound_stream_id: u64,
    outbound_stream_id: u64,
    inbound_streams: HashMap<u64, InboundStream>,
    inbound_stream_timeouts: HashSetDelay<u64>,
    outbound_streams: HashMap<u64, OutboundStream>,
    outbound_stream_timeouts: HashSetDelay<u64>,
    pending_outbound_streams: Vec<OutboundOpenInfo>,
    connection_state: ConnectionState,
}

impl ReqRespConnectionHandler {
    pub fn new(listen_protocol: SubstreamProtocol<InboundReqRespProtocol, ()>) -> Self {
        ReqRespConnectionHandler {
            listen_protocol,
            pending_outbound_streams: vec![],
            behaviour_events: vec![],
            inbound_stream_id: 0,
            outbound_stream_id: 0,
            inbound_streams: HashMap::new(),
            outbound_streams: HashMap::new(),
            connection_state: ConnectionState::Live,
            inbound_stream_timeouts: HashSetDelay::new(REQUEST_TIMEOUT),
            outbound_stream_timeouts: HashSetDelay::new(REQUEST_TIMEOUT),
        }
    }

    fn on_fully_negotiated_inbound(&mut self, inbound_output: InboundOutput<Stream>, _info: ()) {
        let (message, inbound_framed) = inbound_output;

        if let RequestMessage::Goodbye(_) = message {
            self.shutdown();
            return;
        }

        self.inbound_stream_timeouts.insert(self.inbound_stream_id);
        self.inbound_streams.insert(
            self.inbound_stream_id,
            InboundStream {
                state: Some(InboundStreamState::Idle(inbound_framed)),
                response_queue: VecDeque::new(),
            },
        );

        self.behaviour_events.push(HandlerEvent::Ok(Box::new(
            ReqRespMessageReceived::Request {
                stream_id: self.inbound_stream_id,
                message: Box::new(message),
            },
        )));

        self.inbound_stream_id += 1;
    }

    fn on_fully_negotiated_outbound(
        &mut self,
        outbound_output: OutboundFramed<Stream>,
        info: OutboundOpenInfo,
    ) {
        let OutboundOpenInfo {
            request_id,
            message,
        } = info;

        self.outbound_stream_timeouts
            .insert(self.outbound_stream_id);
        self.outbound_streams.insert(
            self.outbound_stream_id,
            OutboundStream {
                state: Some(OutboundStreamState::PendingResponse {
                    stream: Box::new(outbound_output),
                    message,
                }),
                request_id,
            },
        );

        self.outbound_stream_id += 1;
    }

    fn on_dial_upgrade_error(
        &mut self,
        error: StreamUpgradeError<ReqRespError>,
        _info: OutboundOpenInfo,
    ) {
        trace!("REQRESP: Dial upgrade error: {:?}", error);
    }

    fn request(&mut self, request_id: u64, message: RequestMessage) {
        if let ConnectionState::Live = self.connection_state {
            self.pending_outbound_streams.push(OutboundOpenInfo {
                request_id,
                message,
            });
        } else {
            self.behaviour_events
                .push(HandlerEvent::Err(ReqRespMessageError::Outbound {
                    request_id,
                    err: ReqRespError::Disconnected,
                }));
        }
    }

    fn response(&mut self, stream_id: u64, message: RespMessage) {
        let Some(inbound_stream) = self.inbound_streams.get_mut(&stream_id) else {
            error!("REQRESP: Inbound stream not found");
            return;
        };

        if let RespMessage::Error(err) = &message {
            self.behaviour_events
                .push(HandlerEvent::Err(ReqRespMessageError::Inbound {
                    stream_id,
                    err: ReqRespError::RawError(err.to_string()),
                }));
        }

        if let ConnectionState::Closed = self.connection_state {
            return;
        }

        inbound_stream.response_queue.push_back(message);
    }

    fn shutdown(&mut self) {
        if !matches!(
            self.connection_state,
            ConnectionState::ShuttingDown | ConnectionState::Closed
        ) {
            return;
        }

        self.connection_state = ConnectionState::ShuttingDown;
    }
}

impl ConnectionHandler for ReqRespConnectionHandler {
    type FromBehaviour = ConnectionRequest;
    type ToBehaviour = HandlerEvent;
    type InboundProtocol = InboundReqRespProtocol;
    type OutboundProtocol = OutboundReqRespProtocol;
    type InboundOpenInfo = ();
    type OutboundOpenInfo = OutboundOpenInfo;

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        self.listen_protocol.clone()
    }

    fn poll(
        &mut self,
        context: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>,
    > {
        if !self.behaviour_events.is_empty() {
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                self.behaviour_events.remove(0),
            ));
        }

        while let Poll::Ready(Some(Ok(stream_id))) =
            self.inbound_stream_timeouts.poll_expired(context)
        {
            if self.inbound_streams.get_mut(&stream_id).is_some() {
                self.behaviour_events
                    .push(HandlerEvent::Err(ReqRespMessageError::Inbound {
                        stream_id,
                        err: ReqRespError::StreamTimedOut,
                    }));
            }
        }

        while let Poll::Ready(Some(Ok(outbound_id))) =
            self.outbound_stream_timeouts.poll_expired(context)
        {
            if let Some(OutboundStream { request_id, .. }) =
                self.outbound_streams.remove(&outbound_id)
            {
                return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(HandlerEvent::Err(
                    ReqRespMessageError::Outbound {
                        request_id,
                        err: ReqRespError::StreamTimedOut,
                    },
                )));
            }
        }

        let mut streams_to_remove = vec![];
        for (stream_id, inbound_stream) in self.inbound_streams.iter_mut() {
            loop {
                let Some(inbound_stream_state) = inbound_stream.state.take() else {
                    unreachable!(
                        "InboundStreamState should always be present, poll() should not be in parallel"
                    );
                };

                match inbound_stream_state {
                    InboundStreamState::Idle(mut framed) => {
                        if let ConnectionState::Closed = self.connection_state {
                            match framed.close().poll_unpin(context) {
                                Poll::Ready(result) => {
                                    streams_to_remove.push(*stream_id);
                                    self.inbound_stream_timeouts.remove(stream_id);
                                    if let Err(err) = result {
                                        self.behaviour_events.push(HandlerEvent::Err(
                                            ReqRespMessageError::Inbound {
                                                stream_id: *stream_id,
                                                err,
                                            },
                                        ));
                                    }
                                }
                                Poll::Pending => {
                                    inbound_stream.state = Some(InboundStreamState::Idle(framed))
                                }
                            }
                            break;
                        }

                        let Some(response_message) = inbound_stream.response_queue.pop_front()
                        else {
                            inbound_stream.state = Some(InboundStreamState::Idle(framed));
                            break;
                        };

                        inbound_stream.state = Some(InboundStreamState::Busy(Box::pin(
                            send_response_message_to_inbound_stream(framed, response_message)
                                .boxed(),
                        )));
                    }
                    InboundStreamState::Busy(mut pin) => match pin.poll_unpin(context) {
                        Poll::Ready(Ok(framed)) => {
                            let Some(framed) = framed else {
                                streams_to_remove.push(*stream_id);
                                self.inbound_stream_timeouts.remove(stream_id);
                                break;
                            };

                            self.inbound_stream_timeouts.insert(*stream_id);
                            if matches!(self.connection_state, ConnectionState::Closed)
                                || inbound_stream.response_queue.is_empty()
                            {
                                inbound_stream.state = Some(InboundStreamState::Idle(framed));
                                break;
                            }

                            if let Some(response_message) =
                                inbound_stream.response_queue.pop_front()
                            {
                                inbound_stream.state = Some(InboundStreamState::Busy(Box::pin(
                                    send_response_message_to_inbound_stream(
                                        framed,
                                        response_message,
                                    )
                                    .boxed(),
                                )));
                            }
                        }
                        Poll::Ready(Err(err)) => {
                            streams_to_remove.push(*stream_id);
                            self.inbound_stream_timeouts.remove(stream_id);
                            self.behaviour_events.push(HandlerEvent::Err(
                                ReqRespMessageError::Inbound {
                                    stream_id: *stream_id,
                                    err,
                                },
                            ));
                            break;
                        }
                        Poll::Pending => {
                            inbound_stream.state = Some(InboundStreamState::Busy(pin));
                            break;
                        }
                    },
                }
            }
        }

        for stream_id in streams_to_remove {
            self.inbound_streams.remove(&stream_id);
        }

        for stream_id in self.outbound_streams.keys().cloned().collect::<Vec<_>>() {
            let mut entry = match self.outbound_streams.entry(stream_id) {
                Entry::Occupied(entry) => entry,
                Entry::Vacant(_) => {
                    unreachable!(
                        "Outbound stream should always be present, poll() should not be in parallel {stream_id}",
                    );
                }
            };

            let request_id = entry.get().request_id;

            let Some(outbound_stream_state) = entry.get_mut().state.take() else {
                unreachable!(
                    "OutboundStreamState should always be present, poll() should not be in parallel {stream_id}",
                );
            };

            match outbound_stream_state {
                OutboundStreamState::PendingResponse {
                    mut stream,
                    message,
                } => {
                    if let ConnectionState::Closed = self.connection_state {
                        entry.get_mut().state = Some(OutboundStreamState::Closing(stream));
                        self.behaviour_events.push(HandlerEvent::Err(
                            ReqRespMessageError::Outbound {
                                request_id,
                                err: ReqRespError::Disconnected,
                            },
                        ));
                        continue;
                    }

                    match stream.poll_next_unpin(context) {
                        Poll::Ready(response_message) => {
                            let Some(response_message) = response_message else {
                                entry.remove_entry();
                                self.outbound_stream_timeouts.remove(&stream_id);
                                return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                                    HandlerEvent::Ok(Box::new(
                                        ReqRespMessageReceived::EndOfStream { request_id },
                                    )),
                                ));
                            };

                            let response_message = match response_message {
                                Ok(message) => message,
                                Err(err) => {
                                    entry.remove_entry();
                                    self.outbound_stream_timeouts.remove(&stream_id);
                                    return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                                        HandlerEvent::Err(ReqRespMessageError::Outbound {
                                            request_id,
                                            err,
                                        }),
                                    ));
                                }
                            };

                            if matches!(
                                response_message,
                                RespMessage::Error(_) | RespMessage::EndOfStream
                            ) {
                                entry.get_mut().state = Some(OutboundStreamState::Closing(stream));
                            } else {
                                self.outbound_stream_timeouts.insert(stream_id);
                                entry.get_mut().state =
                                    Some(OutboundStreamState::PendingResponse { stream, message });
                            }

                            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                                match response_message {
                                    RespMessage::Response(message) => HandlerEvent::Ok(Box::new(
                                        ReqRespMessageReceived::Response {
                                            request_id,
                                            message,
                                        },
                                    )),
                                    RespMessage::Error(err) => {
                                        HandlerEvent::Err(ReqRespMessageError::Outbound {
                                            request_id,
                                            err,
                                        })
                                    }
                                    RespMessage::EndOfStream => HandlerEvent::Close,
                                },
                            ));
                        }
                        Poll::Pending => {
                            entry.get_mut().state =
                                Some(OutboundStreamState::PendingResponse { stream, message })
                        }
                    }
                }
                OutboundStreamState::Closing(mut stream) => {
                    match Sink::poll_close(Pin::new(&mut stream), context) {
                        Poll::Ready(_) => {
                            entry.remove_entry();
                            self.outbound_stream_timeouts.remove(&stream_id);
                            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                                HandlerEvent::Ok(Box::new(ReqRespMessageReceived::EndOfStream {
                                    request_id,
                                })),
                            ));
                        }
                        Poll::Pending => {
                            entry.get_mut().state = Some(OutboundStreamState::Closing(stream));
                        }
                    }
                }
            }
        }

        if let Some(open_info) = self.pending_outbound_streams.pop() {
            return Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest {
                protocol: SubstreamProtocol::new(
                    OutboundReqRespProtocol {
                        request: open_info.message.clone(),
                    },
                    open_info,
                ),
            });
        }

        if let ConnectionState::ShuttingDown = self.connection_state
            && self.inbound_streams.is_empty()
            && self.outbound_streams.is_empty()
            && self.pending_outbound_streams.is_empty()
            && self.behaviour_events.is_empty()
        {
            self.connection_state = ConnectionState::Closed;
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(HandlerEvent::Close));
        }

        Poll::Pending
    }

    fn on_behaviour_event(&mut self, event: ConnectionRequest) {
        match event {
            ConnectionRequest::Request {
                request_id,
                message,
            } => self.request(request_id, message),
            ConnectionRequest::Response { stream_id, message } => {
                self.response(stream_id, *message)
            }
            ConnectionRequest::Shutdown => self.shutdown(),
        }
    }

    fn on_connection_event(
        &mut self,
        event: ConnectionEvent<
            Self::InboundProtocol,
            Self::OutboundProtocol,
            Self::InboundOpenInfo,
            Self::OutboundOpenInfo,
        >,
    ) {
        match event {
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound { protocol, info }) => {
                self.on_fully_negotiated_inbound(protocol, info)
            }
            ConnectionEvent::FullyNegotiatedOutbound(FullyNegotiatedOutbound {
                protocol,
                info,
            }) => {
                self.on_fully_negotiated_outbound(protocol, info);
            }
            ConnectionEvent::DialUpgradeError(DialUpgradeError { error, info }) => {
                self.on_dial_upgrade_error(error, info);
            }
            // ConnectionEvent is not exhaustive so we have to account for the default case
            _ => (),
        }
    }

    fn connection_keep_alive(&self) -> bool {
        matches!(
            self.connection_state,
            ConnectionState::Live | ConnectionState::ShuttingDown
        )
    }
}

async fn send_response_message_to_inbound_stream(
    mut inbound_stream: InboundFramed<Stream>,
    response_message: RespMessage,
) -> Result<Option<InboundFramed<Stream>>, ReqRespError> {
    if matches!(response_message, RespMessage::EndOfStream) {
        inbound_stream.close().await?;
        return Ok(None);
    }

    let is_error = matches!(response_message, RespMessage::Error(_));
    let result = inbound_stream.send(response_message).await;
    if is_error || result.is_err() {
        inbound_stream.close().await?;
    }

    match result {
        Ok(_) if !is_error => Ok(Some(inbound_stream)),
        Ok(_) => Ok(None),
        Err(err) => Err(err),
    }
}
