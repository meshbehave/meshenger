use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use meshtastic::packet::PacketDestination;
use meshtastic::protobufs;
use meshtastic::types::{MeshChannel, NodeId};
use meshtastic::Message;

use crate::message::{Destination, MessageContext, Response};

use super::runtime::BotPacketRouter;
use super::*;

#[derive(Debug, Clone)]
pub(super) enum OutgoingKind {
    Text,
    Traceroute { target_node: u32 },
}

#[derive(Debug, Clone)]
pub(super) struct OutgoingMeshMessage {
    pub(super) kind: OutgoingKind,
    pub(super) text: String,
    pub(super) destination: PacketDestination,
    pub(super) channel: MeshChannel,
    /// Bot's own node ID (for DB logging as sender)
    pub(super) from_node: u32,
    /// Target node ID for DB logging (None = broadcast)
    pub(super) to_node: Option<u32>,
    /// Meshtastic channel index for DB logging
    pub(super) mesh_channel: u32,
    /// If set, this message is a reply to the incoming packet with this ID
    pub(super) reply_id: Option<u32>,
}

pub(super) struct OutgoingQueue {
    queue: Mutex<VecDeque<OutgoingMeshMessage>>,
    depth: Arc<AtomicUsize>,
}

impl OutgoingQueue {
    pub(super) fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            depth: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub(super) fn depth_handle(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.depth)
    }

    pub(super) fn push(&self, msg: OutgoingMeshMessage) {
        self.queue.lock().unwrap().push_back(msg);
        self.depth.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn pop(&self) -> Option<OutgoingMeshMessage> {
        let msg = self.queue.lock().unwrap().pop_front();
        if msg.is_some() {
            self.depth.fetch_sub(1, Ordering::Relaxed);
        }
        msg
    }

    pub(super) fn is_empty(&self) -> bool {
        self.queue.lock().unwrap().is_empty()
    }

    #[cfg(test)]
    pub(super) fn snapshot(&self) -> Vec<OutgoingMeshMessage> {
        self.queue.lock().unwrap().iter().cloned().collect()
    }
}

impl Bot {
    pub(super) fn queue_responses(
        &self,
        ctx: &MessageContext,
        responses: &[Response],
        my_node_id: u32,
    ) {
        for response in responses {
            let destination = match &response.destination {
                Destination::Sender => PacketDestination::Node(NodeId::from(ctx.sender_id)),
                Destination::Broadcast => PacketDestination::Broadcast,
                Destination::Node(id) => PacketDestination::Node(NodeId::from(*id)),
            };

            let channel = match MeshChannel::new(response.channel) {
                Ok(ch) => ch,
                Err(e) => {
                    log::error!("Invalid channel {}: {}", response.channel, e);
                    continue;
                }
            };

            let to_node = match &response.destination {
                Destination::Sender => Some(ctx.sender_id),
                Destination::Node(id) => Some(*id),
                Destination::Broadcast => None,
            };

            let chunks = chunk_message(&response.text, self.config.bot.max_message_len);
            for (i, chunk) in chunks.into_iter().enumerate() {
                self.queue_message(OutgoingMeshMessage {
                    kind: OutgoingKind::Text,
                    text: chunk,
                    destination,
                    channel,
                    from_node: my_node_id,
                    to_node,
                    mesh_channel: response.channel,
                    // Only the first chunk carries the reply_id
                    reply_id: if i == 0 { response.reply_id } else { None },
                });
            }
        }
    }

    /// Pop and send the next message from the outgoing queue.
    pub(super) async fn send_next_queued_message(
        &self,
        api: &mut meshtastic::api::ConnectedStreamApi,
        router: &mut BotPacketRouter,
    ) {
        let msg = match self.outgoing.pop() {
            Some(m) => m,
            None => return,
        };

        match msg.kind {
            OutgoingKind::Text => {
                if let Some(reply_to_msg_id) = msg.reply_id {
                    log::info!(
                        "Sending queued reply [reply_to_msg_id={}]: {:?} -> {:?}",
                        reply_to_msg_id,
                        msg.text,
                        msg.destination
                    );
                } else {
                    log::info!("Sending queued: {:?} -> {:?}", msg.text, msg.destination);
                }

                // Log outgoing message (no RF metadata for outgoing)
                let _ = self.db.log_packet(
                    msg.from_node,
                    msg.to_node,
                    msg.mesh_channel,
                    &msg.text,
                    "out",
                    false,
                    None,
                    None,
                    None,
                    None,
                    "text",
                );

                let result = if msg.reply_id.is_some() {
                    let byte_data = msg.text.into_bytes().into();
                    api.send_mesh_packet(
                        router,
                        byte_data,
                        protobufs::PortNum::TextMessageApp,
                        msg.destination,
                        msg.channel,
                        true,  // want_ack
                        false, // want_response
                        true,  // echo_response
                        msg.reply_id,
                        None, // emoji
                    )
                    .await
                } else {
                    api.send_text(router, msg.text, msg.destination, true, msg.channel)
                        .await
                };
                if let Err(e) = result {
                    if let Some(reply_to_msg_id) = msg.reply_id {
                        log::error!(
                            "Failed to send queued reply [reply_to_msg_id={}]: {}",
                            reply_to_msg_id,
                            e
                        );
                    } else {
                        log::error!("Failed to send queued message: {}", e);
                    }
                }
            }
            OutgoingKind::Traceroute { target_node } => {
                log::info!("Sending queued traceroute probe to !{:08x}", target_node);
                let _ = self.db.log_packet(
                    msg.from_node,
                    Some(target_node),
                    msg.mesh_channel,
                    "",
                    "out",
                    false,
                    None,
                    None,
                    None,
                    None,
                    "traceroute",
                );

                let routing = protobufs::Routing {
                    variant: Some(protobufs::routing::Variant::RouteRequest(
                        protobufs::RouteDiscovery {
                            route: vec![],
                            snr_towards: vec![],
                            route_back: vec![],
                            snr_back: vec![],
                        },
                    )),
                };
                let payload = routing.encode_to_vec().into();
                let result = api
                    .send_mesh_packet(
                        router,
                        payload,
                        protobufs::PortNum::TracerouteApp,
                        msg.destination,
                        msg.channel,
                        true,  // want_ack
                        true,  // want_response
                        false, // echo_response
                        None,
                        None,
                    )
                    .await;
                if let Err(e) = result {
                    log::error!(
                        "Failed to send queued traceroute to !{:08x}: {}",
                        target_node,
                        e
                    );
                }
            }
        }
    }
}

pub(super) fn chunk_message(text: &str, max_len: usize) -> Vec<String> {
    if max_len == 0 {
        return Vec::new();
    }

    if text.len() <= max_len {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();

    fn split_utf8_by_max_bytes(s: &str, max_len: usize) -> Vec<String> {
        let mut out = Vec::new();
        let mut start = 0;

        while start < s.len() {
            let remaining = &s[start..];
            if remaining.len() <= max_len {
                out.push(remaining.to_string());
                break;
            }

            let mut cut = 0;
            for (idx, ch) in remaining.char_indices() {
                let next = idx + ch.len_utf8();
                if next > max_len {
                    break;
                }
                cut = next;
            }

            // Should never be hit for valid UTF-8 and max_len > 0, but avoid non-progress loops.
            if cut == 0 {
                if let Some(ch) = remaining.chars().next() {
                    cut = ch.len_utf8();
                } else {
                    break;
                }
            }

            out.push(remaining[..cut].to_string());
            start += cut;
        }

        out
    }

    for line in text.lines() {
        // If adding this line would exceed limit, flush current chunk
        if !current.is_empty() && current.len() + 1 + line.len() > max_len {
            chunks.push(current.clone());
            current.clear();
        }

        // If a single line exceeds the limit, split it by characters
        if line.len() > max_len {
            if !current.is_empty() {
                chunks.push(current.clone());
                current.clear();
            }
            let line_chunks = split_utf8_by_max_bytes(line, max_len);
            if !line_chunks.is_empty() {
                for chunk in &line_chunks[..line_chunks.len().saturating_sub(1)] {
                    chunks.push(chunk.clone());
                }
                current = line_chunks.last().cloned().unwrap_or_default();
            }
        } else {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}
