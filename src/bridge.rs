//! Bridge abstraction for connecting mesh to external platforms.

use tokio::sync::{broadcast, mpsc};

/// A message from the mesh network to be forwarded to external platforms.
#[derive(Debug, Clone)]
pub struct MeshBridgeMessage {
    pub sender_id: u32,
    pub sender_name: String,
    pub text: String,
    pub channel: u32,
    pub is_dm: bool,
}

/// A message from an external platform to be sent to the mesh.
#[derive(Debug, Clone)]
pub struct OutgoingBridgeMessage {
    pub text: String,
    pub channel: u32,
    pub source: String, // e.g., "telegram", "discord"
}

/// Sender for mesh messages (bot broadcasts to bridges).
pub type MeshMessageSender = broadcast::Sender<MeshBridgeMessage>;

/// Receiver for mesh messages (bridges receive from bot).
pub type MeshMessageReceiver = broadcast::Receiver<MeshBridgeMessage>;

/// Sender for outgoing messages (bridges send to bot).
pub type OutgoingMessageSender = mpsc::Sender<OutgoingBridgeMessage>;

/// Receiver for outgoing messages (bot receives from bridges).
pub type OutgoingMessageReceiver = mpsc::Receiver<OutgoingBridgeMessage>;

/// Create bridge communication channels.
///
/// Returns:
/// - `MeshMessageSender` - Bot uses this to broadcast mesh messages to all bridges
/// - `OutgoingMessageSender` - Bridges clone this to send messages to the mesh
/// - `OutgoingMessageReceiver` - Bot uses this to receive messages from bridges
pub fn create_bridge_channels() -> (MeshMessageSender, OutgoingMessageSender, OutgoingMessageReceiver) {
    let (mesh_tx, _) = broadcast::channel(100);
    let (outgoing_tx, outgoing_rx) = mpsc::channel(100);
    (mesh_tx, outgoing_tx, outgoing_rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mesh_broadcast_channel() {
        let (tx, _outgoing_tx, _outgoing_rx) = create_bridge_channels();

        let mut rx1 = tx.subscribe();
        let mut rx2 = tx.subscribe();

        let msg = MeshBridgeMessage {
            sender_id: 0x12345678,
            sender_name: "Alice".to_string(),
            text: "Hello".to_string(),
            channel: 0,
            is_dm: false,
        };

        tx.send(msg.clone()).unwrap();

        let received1 = rx1.recv().await.unwrap();
        let received2 = rx2.recv().await.unwrap();

        assert_eq!(received1.text, "Hello");
        assert_eq!(received2.text, "Hello");
    }

    #[tokio::test]
    async fn test_outgoing_channel() {
        let (_mesh_tx, outgoing_tx, mut outgoing_rx) = create_bridge_channels();

        let msg = OutgoingBridgeMessage {
            text: "From Telegram".to_string(),
            channel: 0,
            source: "telegram".to_string(),
        };

        outgoing_tx.send(msg).await.unwrap();

        let received = outgoing_rx.recv().await.unwrap();
        assert_eq!(received.text, "From Telegram");
        assert_eq!(received.source, "telegram");
    }
}
