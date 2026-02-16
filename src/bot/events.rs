use crate::message::{MeshEvent, MessageContext};

use super::*;

impl Bot {
    pub(super) async fn dispatch_deferred_events(
        &self,
        my_node_id: u32,
    ) {
        let events = self.startup_state.take_deferred();

        if events.is_empty() {
            return;
        }

        log::info!(
            "Grace period ended, dispatching {} deferred event(s)",
            events.len()
        );

        for event in &events {
            if let MeshEvent::NodeDiscovered {
                node_id,
                long_name,
                short_name,
                via_mqtt,
            } = event
            {
                self.dispatch_event_to_modules(event, my_node_id).await;

                // Upsert after module dispatch (was deferred along with the event)
                if let Err(e) = self.db.upsert_node(*node_id, short_name, long_name, *via_mqtt) {
                    log::error!("Failed to upsert deferred node: {}", e);
                }
            }
        }
    }

    /// Dispatch an event to all modules, queuing any responses.
    pub(super) async fn dispatch_event_to_modules(&self, event: &MeshEvent, my_node_id: u32) {
        let (node_id, long_name) = match event {
            MeshEvent::NodeDiscovered { node_id, long_name, .. } => (*node_id, long_name.clone()),
            MeshEvent::PositionUpdate { node_id, .. } => (*node_id, String::new()),
        };

        for module in self.registry.all() {
            match module.handle_event(event, &self.db).await {
                Ok(Some(responses)) => {
                    let ctx = MessageContext {
                        sender_id: node_id,
                        sender_name: if !long_name.is_empty() {
                            long_name.clone()
                        } else {
                            format!("!{:08x}", node_id)
                        },
                        channel: 0,
                        is_dm: false,
                        rssi: 0,
                        snr: 0.0,
                        hop_count: 0,
                        hop_limit: 0,
                        via_mqtt: false,
                        packet_id: 0,
                    };
                    self.queue_responses(&ctx, &responses, my_node_id);
                }
                Ok(None) => {}
                Err(e) => {
                    log::error!("Module {} event error: {}", module.name(), e);
                }
            }
        }
    }
}
