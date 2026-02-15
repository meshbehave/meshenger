#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum CommandScope {
    Public,
    DM,
    Both,
}

impl CommandScope {
    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "public" => CommandScope::Public,
            "dm" => CommandScope::DM,
            _ => CommandScope::Both,
        }
    }

    pub fn allows(&self, is_dm: bool) -> bool {
        match self {
            CommandScope::Public => !is_dm,
            CommandScope::DM => is_dm,
            CommandScope::Both => true,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MessageContext {
    pub sender_id: u32,
    pub sender_name: String,
    pub channel: u32,
    pub is_dm: bool,
    pub rssi: i32,
    pub snr: f32,
    pub hop_count: u32,
    pub hop_limit: u32,
    pub via_mqtt: bool,
    /// The incoming mesh packet's unique ID (used for reply threading)
    pub packet_id: u32,
}

#[derive(Debug, Clone)]
pub struct Response {
    pub text: String,
    pub destination: Destination,
    pub channel: u32,
    /// When set, the outgoing message references this incoming packet ID
    pub reply_id: Option<u32>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Destination {
    Sender,
    Broadcast,
    Node(u32),
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum MeshEvent {
    NodeDiscovered {
        node_id: u32,
        long_name: String,
        short_name: String,
        via_mqtt: bool,
    },
    PositionUpdate {
        node_id: u32,
        lat: f64,
        lon: f64,
        altitude: i32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_scope_from_str() {
        assert_eq!(CommandScope::from_str("public"), CommandScope::Public);
        assert_eq!(CommandScope::from_str("PUBLIC"), CommandScope::Public);
        assert_eq!(CommandScope::from_str("dm"), CommandScope::DM);
        assert_eq!(CommandScope::from_str("DM"), CommandScope::DM);
        assert_eq!(CommandScope::from_str("both"), CommandScope::Both);
        assert_eq!(CommandScope::from_str("BOTH"), CommandScope::Both);
        assert_eq!(CommandScope::from_str("unknown"), CommandScope::Both);
    }

    #[test]
    fn test_command_scope_allows_public() {
        let scope = CommandScope::Public;
        assert!(scope.allows(false)); // allows public
        assert!(!scope.allows(true)); // denies DM
    }

    #[test]
    fn test_command_scope_allows_dm() {
        let scope = CommandScope::DM;
        assert!(!scope.allows(false)); // denies public
        assert!(scope.allows(true)); // allows DM
    }

    #[test]
    fn test_command_scope_allows_both() {
        let scope = CommandScope::Both;
        assert!(scope.allows(false)); // allows public
        assert!(scope.allows(true)); // allows DM
    }
}
