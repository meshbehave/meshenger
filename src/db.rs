use chrono::Utc;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;

#[cfg(test)]
use crate::util::parse_node_id;

#[derive(Debug, Clone, Copy)]
pub enum MqttFilter {
    All,
    LocalOnly,
    MqttOnly,
}

impl MqttFilter {
    pub fn from_str(s: &str) -> Self {
        match s {
            "local" => MqttFilter::LocalOnly,
            "mqtt_only" => MqttFilter::MqttOnly,
            _ => MqttFilter::All,
        }
    }

    fn sql_clause(&self) -> &'static str {
        match self {
            MqttFilter::All => "",
            MqttFilter::LocalOnly => " AND via_mqtt = 0",
            MqttFilter::MqttOnly => " AND via_mqtt = 1",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DashboardOverview {
    pub node_count: u64,
    pub messages_in: u64,
    pub messages_out: u64,
    pub packets_in: u64,
    pub packets_out: u64,
    pub bot_name: String,
}

#[derive(Debug, Serialize)]
pub struct DashboardNode {
    pub node_id: String,
    pub short_name: String,
    pub long_name: String,
    pub last_seen: i64,
    pub last_rf_seen: Option<i64>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub via_mqtt: bool,
    pub last_hop: Option<u32>,
    pub min_hop: Option<u32>,
    pub avg_hop: Option<f64>,
    pub hop_samples: u32,
}

#[derive(Debug, Serialize)]
pub struct ThroughputBucket {
    pub hour: String,
    pub incoming: u64,
    pub outgoing: u64,
}

#[derive(Debug, Serialize)]
pub struct DistributionBucket {
    pub label: String,
    pub count: u64,
}

#[derive(Debug, Serialize)]
pub struct TracerouteRequester {
    pub node_id: String,
    pub short_name: String,
    pub long_name: String,
    pub request_count: u64,
    pub last_request: i64,
    pub via_mqtt: bool,
}

#[derive(Debug, Serialize)]
pub struct TracerouteEvent {
    pub timestamp: i64,
    pub from_node: String,
    pub from_short_name: String,
    pub from_long_name: String,
    pub to_node: String,
    pub to_short_name: String,
    pub to_long_name: String,
    pub via_mqtt: bool,
    pub hop_count: Option<u32>,
    pub hop_start: Option<u32>,
    pub rssi: Option<i32>,
    pub snr: Option<f32>,
}

#[derive(Debug, Serialize)]
pub struct TracerouteDestinationSummary {
    pub destination_node: String,
    pub destination_short_name: String,
    pub destination_long_name: String,
    pub requests: u64,
    pub unique_requesters: u64,
    pub last_seen: i64,
    pub rf_count: u64,
    pub mqtt_count: u64,
    pub avg_hops: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct HopsToMeRow {
    pub source_node: String,
    pub source_short_name: String,
    pub source_long_name: String,
    pub samples: u64,
    pub last_seen: i64,
    pub last_hops: Option<u32>,
    pub min_hops: Option<u32>,
    pub avg_hops: Option<f64>,
    pub max_hops: Option<u32>,
    pub rf_count: u64,
    pub mqtt_count: u64,
}

#[derive(Debug, Serialize)]
pub struct TracerouteSessionRow {
    pub id: i64,
    pub trace_key: String,
    pub src_node: String,
    pub src_short_name: String,
    pub src_long_name: String,
    pub dst_node: String,
    pub dst_short_name: String,
    pub dst_long_name: String,
    pub first_seen: i64,
    pub last_seen: i64,
    pub via_mqtt: bool,
    pub request_hops: Option<u32>,
    pub request_hop_start: Option<u32>,
    pub response_hops: Option<u32>,
    pub response_hop_start: Option<u32>,
    pub status: String,
    pub sample_count: u64,
}

#[derive(Debug, Serialize)]
pub struct TracerouteSessionHop {
    pub direction: String,
    pub hop_index: u32,
    pub node_id: String,
    pub observed_at: i64,
    pub source_kind: String,
}

#[derive(Debug, Serialize)]
pub struct TracerouteSessionDetail {
    pub session: TracerouteSessionRow,
    pub hops: Vec<TracerouteSessionHop>,
}

pub struct Db {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Node {
    pub node_id: u32,
    pub short_name: String,
    pub long_name: String,
    pub first_seen: i64,
    pub last_seen: i64,
    pub last_welcomed: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct NodeWithHop {
    pub node_id: u32,
    pub short_name: String,
    pub long_name: String,
    pub last_seen: i64,
    pub last_hop: Option<u32>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let conn = Connection::open(path)?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS nodes (
                node_id        INTEGER PRIMARY KEY,
                short_name     TEXT NOT NULL DEFAULT '',
                long_name      TEXT NOT NULL DEFAULT '',
                first_seen     INTEGER NOT NULL,
                last_seen      INTEGER NOT NULL,
                last_welcomed  INTEGER,
                latitude       REAL,
                longitude      REAL,
                via_mqtt       INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS packets (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp  INTEGER NOT NULL,
                from_node  INTEGER NOT NULL,
                to_node    INTEGER,
                channel    INTEGER NOT NULL,
                text       TEXT NOT NULL,
                direction  TEXT NOT NULL,
                via_mqtt   INTEGER NOT NULL DEFAULT 0,
                rssi       INTEGER,
                snr        REAL,
                hop_count  INTEGER,
                hop_start  INTEGER,
                mesh_packet_id INTEGER,
                packet_type TEXT NOT NULL DEFAULT 'text'
            );

            CREATE TABLE IF NOT EXISTS mail (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp  INTEGER NOT NULL,
                from_node  INTEGER NOT NULL,
                to_node    INTEGER NOT NULL,
                body       TEXT NOT NULL,
                read       INTEGER NOT NULL DEFAULT 0
            );

            CREATE INDEX IF NOT EXISTS idx_packets_rf_hops_lookup
            ON packets (from_node, direction, via_mqtt, timestamp DESC, id DESC)
            WHERE hop_count IS NOT NULL;

            CREATE INDEX IF NOT EXISTS idx_packets_rf_last_seen
            ON packets (from_node, direction, via_mqtt, timestamp DESC, id DESC);

            CREATE INDEX IF NOT EXISTS idx_packets_rf_hops_stats
            ON packets (direction, via_mqtt, from_node, hop_count)
            WHERE hop_count IS NOT NULL;",
        )?;

        let has_mesh_packet_id: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('packets') WHERE name = 'mesh_packet_id'",
            [],
            |row| row.get(0),
        )?;
        if has_mesh_packet_id == 0 {
            conn.execute("ALTER TABLE packets ADD COLUMN mesh_packet_id INTEGER", [])?;
        }

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS traceroute_sessions (
                id                 INTEGER PRIMARY KEY AUTOINCREMENT,
                trace_key          TEXT NOT NULL UNIQUE,
                first_seen         INTEGER NOT NULL,
                last_seen          INTEGER NOT NULL,
                src_node           INTEGER NOT NULL,
                dst_node           INTEGER,
                via_mqtt           INTEGER NOT NULL DEFAULT 0,
                request_hops       INTEGER,
                request_hop_start  INTEGER,
                response_hops      INTEGER,
                response_hop_start INTEGER,
                request_packet_id  INTEGER,
                response_packet_id INTEGER,
                status             TEXT NOT NULL DEFAULT 'observed',
                sample_count       INTEGER NOT NULL DEFAULT 1,
                FOREIGN KEY(request_packet_id) REFERENCES packets(id) ON DELETE SET NULL,
                FOREIGN KEY(response_packet_id) REFERENCES packets(id) ON DELETE SET NULL
            );

            CREATE TABLE IF NOT EXISTS traceroute_session_hops (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id    INTEGER NOT NULL,
                direction     TEXT NOT NULL,
                hop_index     INTEGER NOT NULL,
                node_id       INTEGER NOT NULL,
                observed_at   INTEGER NOT NULL,
                packet_id_ref INTEGER,
                source_kind   TEXT NOT NULL DEFAULT 'route',
                FOREIGN KEY(session_id) REFERENCES traceroute_sessions(id) ON DELETE CASCADE,
                FOREIGN KEY(packet_id_ref) REFERENCES packets(id) ON DELETE SET NULL
            );

            CREATE INDEX IF NOT EXISTS idx_tr_sessions_last_seen
            ON traceroute_sessions (last_seen DESC, id DESC);

            CREATE INDEX IF NOT EXISTS idx_tr_sessions_src_dst
            ON traceroute_sessions (src_node, dst_node, last_seen DESC);

            CREATE INDEX IF NOT EXISTS idx_tr_hops_session
            ON traceroute_session_hops (session_id, direction, hop_index);

            CREATE INDEX IF NOT EXISTS idx_tr_hops_packet_ref
            ON traceroute_session_hops (packet_id_ref);",
        )?;

        Ok(())
    }

    pub fn upsert_node(
        &self,
        node_id: u32,
        short_name: &str,
        long_name: &str,
        via_mqtt: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();
        conn.execute(
            "INSERT INTO nodes (node_id, short_name, long_name, first_seen, last_seen, via_mqtt)
             VALUES (?1, ?2, ?3, ?4, ?4, ?5)
             ON CONFLICT(node_id) DO UPDATE SET
                short_name = CASE WHEN ?2 != '' THEN ?2 ELSE short_name END,
                long_name  = CASE WHEN ?3 != '' THEN ?3 ELSE long_name END,
                last_seen  = ?4,
                via_mqtt   = ?5",
            params![node_id as i64, short_name, long_name, now, via_mqtt as i64],
        )?;
        Ok(())
    }

    pub fn is_node_new(
        &self,
        node_id: u32,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE node_id = ?1",
            params![node_id as i64],
            |row| row.get(0),
        )?;
        Ok(count == 0)
    }

    pub fn is_node_absent(
        &self,
        node_id: u32,
        threshold_hours: u64,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let threshold = Utc::now().timestamp() - (threshold_hours as i64 * 3600);
        let result: Result<i64, _> = conn.query_row(
            "SELECT last_seen FROM nodes WHERE node_id = ?1",
            params![node_id as i64],
            |row| row.get(0),
        );
        match result {
            Ok(last_seen) => Ok(last_seen < threshold),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(true),
            Err(e) => Err(e.into()),
        }
    }

    pub fn mark_welcomed(
        &self,
        node_id: u32,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();
        conn.execute(
            "UPDATE nodes SET last_welcomed = ?1 WHERE node_id = ?2",
            params![now, node_id as i64],
        )?;
        Ok(())
    }

    #[cfg(test)]
    pub fn get_all_nodes(&self) -> Result<Vec<Node>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT node_id, short_name, long_name, first_seen, last_seen, last_welcomed
             FROM nodes ORDER BY last_seen DESC",
        )?;
        let nodes = stmt
            .query_map([], |row| {
                Ok(Node {
                    node_id: row.get::<_, i64>(0)? as u32,
                    short_name: row.get(1)?,
                    long_name: row.get(2)?,
                    first_seen: row.get(3)?,
                    last_seen: row.get(4)?,
                    last_welcomed: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(nodes)
    }

    pub fn get_recent_nodes_with_last_hop(
        &self,
        limit: usize,
    ) -> Result<Vec<NodeWithHop>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT
                n.node_id,
                n.short_name,
                n.long_name,
                n.last_seen,
                (
                    SELECT p.hop_count
                    FROM packets p
                    WHERE p.from_node = n.node_id
                      AND p.direction = 'in'
                      AND p.via_mqtt = 0
                      AND p.hop_count IS NOT NULL
                    ORDER BY p.timestamp DESC, p.id DESC
                    LIMIT 1
                ) AS last_hop
             FROM nodes n
             ORDER BY n.last_seen DESC
             LIMIT ?1",
        )?;
        let nodes = stmt
            .query_map(params![limit as i64], |row| {
                Ok(NodeWithHop {
                    node_id: row.get::<_, i64>(0)? as u32,
                    short_name: row.get(1)?,
                    long_name: row.get(2)?,
                    last_seen: row.get(3)?,
                    last_hop: row.get::<_, Option<i64>>(4)?.map(|h| h as u32),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(nodes)
    }

    pub fn get_node_name(
        &self,
        node_id: u32,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let result: Result<(String, String), _> = conn.query_row(
            "SELECT long_name, short_name FROM nodes WHERE node_id = ?1",
            params![node_id as i64],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );
        match result {
            Ok((long, short)) => {
                if !long.is_empty() {
                    Ok(long)
                } else if !short.is_empty() {
                    Ok(short)
                } else {
                    Ok(format!("!{:08x}", node_id))
                }
            }
            Err(_) => Ok(format!("!{:08x}", node_id)),
        }
    }

    pub fn update_position(
        &self,
        node_id: u32,
        lat: f64,
        lon: f64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();
        conn.execute(
            "UPDATE nodes SET latitude = ?1, longitude = ?2, last_seen = ?3 WHERE node_id = ?4",
            params![lat, lon, now, node_id as i64],
        )?;
        Ok(())
    }

    pub fn purge_nodes_not_seen_within(
        &self,
        max_age_secs: u64,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        let max_age_secs = i64::try_from(max_age_secs)
            .map_err(|_| "max_age_secs too large for timestamp arithmetic")?;
        let cutoff = Utc::now().timestamp() - max_age_secs;
        let conn = self.conn.lock().unwrap();
        let deleted = conn.execute("DELETE FROM nodes WHERE last_seen < ?1", params![cutoff])?;
        Ok(deleted)
    }

    pub fn get_node_position(
        &self,
        node_id: u32,
    ) -> Result<Option<(f64, f64)>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let result: Result<(Option<f64>, Option<f64>), _> = conn.query_row(
            "SELECT latitude, longitude FROM nodes WHERE node_id = ?1",
            params![node_id as i64],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );
        match result {
            Ok((Some(lat), Some(lon))) if lat != 0.0 || lon != 0.0 => Ok(Some((lat, lon))),
            _ => Ok(None),
        }
    }

    pub fn message_count(
        &self,
        direction: &str,
    ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM packets WHERE direction = ?1",
            params![direction],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    pub fn node_count(&self) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM nodes", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    #[cfg(test)]
    pub fn find_node_by_name(
        &self,
        name: &str,
    ) -> Result<Option<u32>, Box<dyn std::error::Error + Send + Sync>> {
        // Try parsing as node ID (hex with/without prefix, or decimal)
        if let Some(id) = parse_node_id(name) {
            let conn = self.conn.lock().unwrap();
            let exists: i64 = conn.query_row(
                "SELECT COUNT(*) FROM nodes WHERE node_id = ?1",
                params![id as i64],
                |row| row.get(0),
            )?;
            if exists > 0 {
                return Ok(Some(id));
            }
        }

        // Try matching by short_name or long_name (case-insensitive)
        let conn = self.conn.lock().unwrap();
        let result: Result<i64, _> = conn.query_row(
            "SELECT node_id FROM nodes WHERE lower(short_name) = lower(?1) OR lower(long_name) = lower(?1) LIMIT 1",
            params![name],
            |row| row.get(0),
        );
        match result {
            Ok(id) => Ok(Some(id as u32)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Return the most recently seen RF node (within `max_age_secs`) that has no inbound RF hop metadata recorded.
    pub fn recent_rf_node_missing_hops(
        &self,
        max_age_secs: u64,
        exclude_node_id: Option<u32>,
    ) -> Result<Option<u32>, Box<dyn std::error::Error + Send + Sync>> {
        let candidates =
            self.recent_rf_nodes_missing_hops(max_age_secs, exclude_node_id, 1usize)?;
        Ok(candidates.into_iter().next())
    }

    /// Return up to `limit` most recently seen RF nodes missing inbound RF hop metadata.
    pub fn recent_rf_nodes_missing_hops(
        &self,
        max_age_secs: u64,
        exclude_node_id: Option<u32>,
        limit: usize,
    ) -> Result<Vec<u32>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let since = Utc::now().timestamp() - (max_age_secs as i64);
        let exclude = exclude_node_id.unwrap_or(0) as i64;
        let mut stmt = conn.prepare(
            "SELECT n.node_id
             FROM nodes n
             WHERE n.via_mqtt = 0
               AND n.last_seen > ?1
               AND (?2 = 0 OR n.node_id != ?2)
               AND NOT EXISTS (
                   SELECT 1
                   FROM packets p
                   WHERE p.from_node = n.node_id
                     AND p.direction = 'in'
                     AND p.via_mqtt = 0
                     AND p.hop_count IS NOT NULL
               )
             ORDER BY n.last_seen DESC
             LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(params![since, exclude, limit as i64], |row| {
                row.get::<_, i64>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows.into_iter().map(|id| id as u32).collect())
    }

    // --- Packet logging ---

    #[allow(clippy::too_many_arguments)]
    fn log_packet_inner(
        &self,
        from_node: u32,
        to_node: Option<u32>,
        channel: u32,
        text: &str,
        direction: &str,
        via_mqtt: bool,
        rssi: Option<i32>,
        snr: Option<f32>,
        hop_count: Option<u32>,
        hop_start: Option<u32>,
        mesh_packet_id: Option<u32>,
        packet_type: &str,
    ) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();
        conn.execute(
            "INSERT INTO packets (timestamp, from_node, to_node, channel, text, direction, via_mqtt, rssi, snr, hop_count, hop_start, mesh_packet_id, packet_type)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                now,
                from_node as i64,
                to_node.map(|n| n as i64),
                channel as i64,
                text,
                direction,
                via_mqtt as i64,
                rssi,
                snr,
                hop_count.map(|h| h as i64),
                hop_start.map(|h| h as i64),
                mesh_packet_id.map(|m| m as i64),
                packet_type,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn log_packet(
        &self,
        from_node: u32,
        to_node: Option<u32>,
        channel: u32,
        text: &str,
        direction: &str,
        via_mqtt: bool,
        rssi: Option<i32>,
        snr: Option<f32>,
        hop_count: Option<u32>,
        hop_start: Option<u32>,
        packet_type: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.log_packet_inner(
            from_node,
            to_node,
            channel,
            text,
            direction,
            via_mqtt,
            rssi,
            snr,
            hop_count,
            hop_start,
            None,
            packet_type,
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn log_packet_with_mesh_id(
        &self,
        from_node: u32,
        to_node: Option<u32>,
        channel: u32,
        text: &str,
        direction: &str,
        via_mqtt: bool,
        rssi: Option<i32>,
        snr: Option<f32>,
        hop_count: Option<u32>,
        hop_start: Option<u32>,
        mesh_packet_id: Option<u32>,
        packet_type: &str,
    ) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        self.log_packet_inner(
            from_node,
            to_node,
            channel,
            text,
            direction,
            via_mqtt,
            rssi,
            snr,
            hop_count,
            hop_start,
            mesh_packet_id,
            packet_type,
        )
    }

    // --- Dashboard queries ---

    pub fn dashboard_overview(
        &self,
        hours: u32,
        filter: MqttFilter,
        bot_name: &str,
    ) -> Result<DashboardOverview, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let node_count: i64 = conn.query_row("SELECT COUNT(*) FROM nodes", [], |row| row.get(0))?;
        let since = if hours == 0 {
            0
        } else {
            Utc::now().timestamp() - (hours as i64 * 3600)
        };

        let mqtt_clause = filter.sql_clause();

        // Text messages only
        let query_msg_in = format!(
            "SELECT COUNT(*) FROM packets WHERE direction = 'in' AND packet_type = 'text' AND timestamp > ?1{}",
            mqtt_clause
        );
        let messages_in: i64 = conn.query_row(&query_msg_in, params![since], |row| row.get(0))?;

        let query_msg_out = format!(
            "SELECT COUNT(*) FROM packets WHERE direction = 'out' AND packet_type = 'text' AND timestamp > ?1{}",
            mqtt_clause
        );
        let messages_out: i64 = conn.query_row(&query_msg_out, params![since], |row| row.get(0))?;

        // All packet types
        let query_pkt_in = format!(
            "SELECT COUNT(*) FROM packets WHERE direction = 'in' AND timestamp > ?1{}",
            mqtt_clause
        );
        let packets_in: i64 = conn.query_row(&query_pkt_in, params![since], |row| row.get(0))?;

        let query_pkt_out = format!(
            "SELECT COUNT(*) FROM packets WHERE direction = 'out' AND timestamp > ?1{}",
            mqtt_clause
        );
        let packets_out: i64 = conn.query_row(&query_pkt_out, params![since], |row| row.get(0))?;

        Ok(DashboardOverview {
            node_count: node_count as u64,
            messages_in: messages_in as u64,
            messages_out: messages_out as u64,
            packets_in: packets_in as u64,
            packets_out: packets_out as u64,
            bot_name: bot_name.to_string(),
        })
    }

    pub fn dashboard_nodes(
        &self,
        hours: u32,
        filter: MqttFilter,
    ) -> Result<Vec<DashboardNode>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let since = if hours == 0 {
            0
        } else {
            Utc::now().timestamp() - (hours as i64 * 3600)
        };

        let where_clause = match filter {
            MqttFilter::All => String::new(),
            MqttFilter::LocalOnly => " WHERE n.via_mqtt = 0".to_string(),
            MqttFilter::MqttOnly => " WHERE n.via_mqtt = 1".to_string(),
        };

        let query = format!(
            "WITH rf_last AS (
                SELECT
                    from_node,
                    timestamp,
                    ROW_NUMBER() OVER (PARTITION BY from_node ORDER BY timestamp DESC, id DESC) AS rn
                FROM packets
                WHERE direction = 'in' AND via_mqtt = 0
             ),
             rf_hops AS (
                SELECT
                    from_node,
                    hop_count,
                    ROW_NUMBER() OVER (PARTITION BY from_node ORDER BY timestamp DESC, id DESC) AS rn
                FROM packets
                WHERE direction = 'in' AND via_mqtt = 0 AND hop_count IS NOT NULL
             ),
             rf_stats AS (
                SELECT
                    from_node,
                    MIN(hop_count) AS min_hop,
                    AVG(hop_count) AS avg_hop,
                    COUNT(*) AS hop_samples
                FROM packets
                WHERE direction = 'in' AND via_mqtt = 0 AND hop_count IS NOT NULL
                  AND timestamp > ?1
                GROUP BY from_node
             )
             SELECT
                n.node_id, n.short_name, n.long_name, n.last_seen, lr.timestamp AS last_rf_seen, n.latitude, n.longitude, n.via_mqtt,
                lh.hop_count AS last_hop,
                rs.min_hop,
                rs.avg_hop,
                COALESCE(rs.hop_samples, 0) AS hop_samples
             FROM nodes n
             LEFT JOIN rf_last lr ON lr.from_node = n.node_id AND lr.rn = 1
             LEFT JOIN rf_hops lh ON lh.from_node = n.node_id AND lh.rn = 1
             LEFT JOIN rf_stats rs ON rs.from_node = n.node_id
             {} ORDER BY n.last_seen DESC",
            where_clause
        );
        let mut stmt = conn.prepare(&query)?;
        let nodes = stmt
            .query_map(params![since], |row| {
                let nid: i64 = row.get(0)?;
                let via_mqtt_val: i64 = row.get(7)?;
                let last_hop: Option<i64> = row.get(8)?;
                let min_hop: Option<i64> = row.get(9)?;
                let avg_hop: Option<f64> = row.get(10)?;
                let hop_samples: i64 = row.get(11)?;
                Ok(DashboardNode {
                    node_id: format!("!{:08x}", nid as u32),
                    short_name: row.get(1)?,
                    long_name: row.get(2)?,
                    last_seen: row.get(3)?,
                    last_rf_seen: row.get(4)?,
                    latitude: row.get(5)?,
                    longitude: row.get(6)?,
                    via_mqtt: via_mqtt_val != 0,
                    last_hop: last_hop.map(|h| h as u32),
                    min_hop: min_hop.map(|h| h as u32),
                    avg_hop,
                    hop_samples: hop_samples as u32,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(nodes)
    }

    /// Throughput of text messages only (existing chart).
    pub fn dashboard_throughput(
        &self,
        hours: u32,
        filter: MqttFilter,
    ) -> Result<Vec<ThroughputBucket>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let since = if hours == 0 {
            0
        } else {
            Utc::now().timestamp() - (hours as i64 * 3600)
        };

        let bucket_expr = if hours > 48 {
            "strftime('%Y-%m-%d', timestamp, 'unixepoch')"
        } else {
            "strftime('%Y-%m-%d %H:00', timestamp, 'unixepoch')"
        };

        let query = format!(
            "SELECT
                {bucket} AS bucket,
                SUM(CASE WHEN direction = 'in' THEN 1 ELSE 0 END) AS incoming,
                SUM(CASE WHEN direction = 'out' THEN 1 ELSE 0 END) AS outgoing
             FROM packets
             WHERE packet_type = 'text' AND timestamp > ?1{mqtt}
             GROUP BY bucket
             ORDER BY bucket",
            bucket = bucket_expr,
            mqtt = filter.sql_clause()
        );
        let mut stmt = conn.prepare(&query)?;
        let buckets = stmt
            .query_map(params![since], |row| {
                Ok(ThroughputBucket {
                    hour: row.get(0)?,
                    incoming: row.get::<_, i64>(1)? as u64,
                    outgoing: row.get::<_, i64>(2)? as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(buckets)
    }

    /// Throughput of all or filtered packet types.
    pub fn dashboard_packet_throughput(
        &self,
        hours: u32,
        filter: MqttFilter,
        packet_types: Option<&[String]>,
    ) -> Result<Vec<ThroughputBucket>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let since = if hours == 0 {
            0
        } else {
            Utc::now().timestamp() - (hours as i64 * 3600)
        };

        let bucket_expr = if hours > 48 {
            "strftime('%Y-%m-%d', timestamp, 'unixepoch')"
        } else {
            "strftime('%Y-%m-%d %H:00', timestamp, 'unixepoch')"
        };

        const VALID_PACKET_TYPES: &[&str] = &[
            "text",
            "position",
            "telemetry",
            "nodeinfo",
            "traceroute",
            "neighborinfo",
            "routing",
            "other",
        ];

        let type_clause = match packet_types {
            Some(types) if !types.is_empty() => {
                let safe: Vec<&&str> = types
                    .iter()
                    .filter_map(|t| VALID_PACKET_TYPES.iter().find(|&&v| v == t.as_str()))
                    .collect();
                if safe.is_empty() {
                    return Ok(vec![]);
                }
                let placeholders: Vec<String> = safe.iter().map(|t| format!("'{}'", t)).collect();
                format!(" AND packet_type IN ({})", placeholders.join(","))
            }
            _ => String::new(),
        };

        let query = format!(
            "SELECT
                {bucket} AS bucket,
                SUM(CASE WHEN direction = 'in' THEN 1 ELSE 0 END) AS incoming,
                SUM(CASE WHEN direction = 'out' THEN 1 ELSE 0 END) AS outgoing
             FROM packets
             WHERE timestamp > ?1{mqtt}{types}
             GROUP BY bucket
             ORDER BY bucket",
            bucket = bucket_expr,
            mqtt = filter.sql_clause(),
            types = type_clause,
        );
        let mut stmt = conn.prepare(&query)?;
        let buckets = stmt
            .query_map(params![since], |row| {
                Ok(ThroughputBucket {
                    hour: row.get(0)?,
                    incoming: row.get::<_, i64>(1)? as u64,
                    outgoing: row.get::<_, i64>(2)? as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(buckets)
    }

    pub fn dashboard_rssi(
        &self,
        hours: u32,
        filter: MqttFilter,
    ) -> Result<Vec<DistributionBucket>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let since = if hours == 0 {
            0
        } else {
            Utc::now().timestamp() - (hours as i64 * 3600)
        };

        // Bucket RSSI into 10 dBm ranges
        let query = format!(
            "SELECT
                (rssi / 10) * 10 AS bucket,
                COUNT(*) AS cnt
             FROM packets
             WHERE direction = 'in' AND rssi IS NOT NULL AND timestamp > ?1{}
             GROUP BY bucket
             ORDER BY bucket",
            filter.sql_clause()
        );
        let mut stmt = conn.prepare(&query)?;
        let buckets = stmt
            .query_map(params![since], |row| {
                let bucket: i32 = row.get(0)?;
                Ok(DistributionBucket {
                    label: format!("{} dBm", bucket),
                    count: row.get::<_, i64>(1)? as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(buckets)
    }

    pub fn dashboard_snr(
        &self,
        hours: u32,
        filter: MqttFilter,
    ) -> Result<Vec<DistributionBucket>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let since = if hours == 0 {
            0
        } else {
            Utc::now().timestamp() - (hours as i64 * 3600)
        };

        // Bucket SNR into 2.5 dB ranges
        let query = format!(
            "SELECT
                CAST(ROUND(snr / 2.5) * 2.5 AS TEXT) AS bucket,
                COUNT(*) AS cnt
             FROM packets
             WHERE direction = 'in' AND snr IS NOT NULL AND timestamp > ?1{}
             GROUP BY bucket
             ORDER BY CAST(bucket AS REAL)",
            filter.sql_clause()
        );
        let mut stmt = conn.prepare(&query)?;
        let buckets = stmt
            .query_map(params![since], |row| {
                let bucket: String = row.get(0)?;
                Ok(DistributionBucket {
                    label: format!("{} dB", bucket),
                    count: row.get::<_, i64>(1)? as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(buckets)
    }

    pub fn dashboard_hops(
        &self,
        hours: u32,
        filter: MqttFilter,
    ) -> Result<Vec<DistributionBucket>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let since = if hours == 0 {
            0
        } else {
            Utc::now().timestamp() - (hours as i64 * 3600)
        };

        let query = format!(
            "SELECT
                hop_count,
                COUNT(*) AS cnt
             FROM packets
             WHERE direction = 'in' AND hop_count IS NOT NULL AND timestamp > ?1{}
             GROUP BY hop_count
             ORDER BY hop_count",
            filter.sql_clause()
        );
        let mut stmt = conn.prepare(&query)?;
        let buckets = stmt
            .query_map(params![since], |row| {
                let hops: i32 = row.get(0)?;
                Ok(DistributionBucket {
                    label: format!("{} hop{}", hops, if hops == 1 { "" } else { "s" }),
                    count: row.get::<_, i64>(1)? as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(buckets)
    }

    pub fn dashboard_positions(
        &self,
    ) -> Result<Vec<DashboardNode>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "WITH rf_last AS (
                SELECT
                    from_node,
                    timestamp,
                    ROW_NUMBER() OVER (PARTITION BY from_node ORDER BY timestamp DESC, id DESC) AS rn
                FROM packets
                WHERE direction = 'in' AND via_mqtt = 0
             ),
             rf_hops AS (
                SELECT
                    from_node,
                    hop_count,
                    ROW_NUMBER() OVER (PARTITION BY from_node ORDER BY timestamp DESC, id DESC) AS rn
                FROM packets
                WHERE direction = 'in' AND via_mqtt = 0 AND hop_count IS NOT NULL
             ),
             rf_stats AS (
                SELECT
                    from_node,
                    MIN(hop_count) AS min_hop,
                    AVG(hop_count) AS avg_hop,
                    COUNT(*) AS hop_samples
                FROM packets
                WHERE direction = 'in' AND via_mqtt = 0 AND hop_count IS NOT NULL
                GROUP BY from_node
             )
             SELECT
                n.node_id, n.short_name, n.long_name, n.last_seen, lr.timestamp AS last_rf_seen, n.latitude, n.longitude, n.via_mqtt,
                lh.hop_count AS last_hop,
                rs.min_hop,
                rs.avg_hop,
                COALESCE(rs.hop_samples, 0) AS hop_samples
             FROM nodes n
             LEFT JOIN rf_last lr ON lr.from_node = n.node_id AND lr.rn = 1
             LEFT JOIN rf_hops lh ON lh.from_node = n.node_id AND lh.rn = 1
             LEFT JOIN rf_stats rs ON rs.from_node = n.node_id
             WHERE n.latitude IS NOT NULL AND n.longitude IS NOT NULL
               AND (n.latitude != 0.0 OR n.longitude != 0.0)
             ORDER BY n.last_seen DESC",
        )?;
        let nodes = stmt
            .query_map([], |row| {
                let nid: i64 = row.get(0)?;
                let via_mqtt_val: i64 = row.get(7)?;
                let last_hop: Option<i64> = row.get(8)?;
                let min_hop: Option<i64> = row.get(9)?;
                let avg_hop: Option<f64> = row.get(10)?;
                let hop_samples: i64 = row.get(11)?;
                Ok(DashboardNode {
                    node_id: format!("!{:08x}", nid as u32),
                    short_name: row.get(1)?,
                    long_name: row.get(2)?,
                    last_seen: row.get(3)?,
                    last_rf_seen: row.get(4)?,
                    latitude: row.get(5)?,
                    longitude: row.get(6)?,
                    via_mqtt: via_mqtt_val != 0,
                    last_hop: last_hop.map(|h| h as u32),
                    min_hop: min_hop.map(|h| h as u32),
                    avg_hop,
                    hop_samples: hop_samples as u32,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(nodes)
    }

    pub fn dashboard_traceroute_requesters(
        &self,
        target_node: u32,
        hours: u32,
        filter: MqttFilter,
    ) -> Result<Vec<TracerouteRequester>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let since = if hours == 0 {
            0
        } else {
            Utc::now().timestamp() - (hours as i64 * 3600)
        };

        let mqtt_clause = match filter {
            MqttFilter::All => "",
            MqttFilter::LocalOnly => " AND p.via_mqtt = 0",
            MqttFilter::MqttOnly => " AND p.via_mqtt = 1",
        };

        let query = format!(
            "SELECT
                p.from_node,
                COALESCE(n.short_name, '') AS short_name,
                COALESCE(n.long_name, '') AS long_name,
                COUNT(*) AS request_count,
                MAX(p.timestamp) AS last_request,
                MAX(p.via_mqtt) AS via_mqtt
             FROM packets p
             LEFT JOIN nodes n ON n.node_id = p.from_node
             WHERE p.direction = 'in'
               AND p.packet_type = 'traceroute'
               AND p.to_node = ?1
               AND p.timestamp > ?2
               {mqtt_clause}
             GROUP BY p.from_node, n.short_name, n.long_name
             ORDER BY last_request DESC"
        );

        let rows = conn
            .prepare(&query)?
            .query_map(params![target_node as i64, since], |row| {
                let node_id_i64: i64 = row.get(0)?;
                let short_name: String = row.get(1)?;
                let long_name: String = row.get(2)?;
                let request_count: i64 = row.get(3)?;
                let last_request: i64 = row.get(4)?;
                let via_mqtt: i64 = row.get(5)?;
                Ok(TracerouteRequester {
                    node_id: format!("!{:08x}", node_id_i64 as u32),
                    short_name,
                    long_name,
                    request_count: request_count as u64,
                    last_request,
                    via_mqtt: via_mqtt != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    pub fn dashboard_traceroute_events(
        &self,
        hours: u32,
        filter: MqttFilter,
        limit: u32,
    ) -> Result<Vec<TracerouteEvent>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let since = if hours == 0 {
            0
        } else {
            Utc::now().timestamp() - (hours as i64 * 3600)
        };
        let mqtt_clause = match filter {
            MqttFilter::All => "",
            MqttFilter::LocalOnly => " AND p.via_mqtt = 0",
            MqttFilter::MqttOnly => " AND p.via_mqtt = 1",
        };

        let query = format!(
            "SELECT
                p.timestamp,
                p.from_node,
                COALESCE(nf.short_name, '') AS from_short_name,
                COALESCE(nf.long_name, '') AS from_long_name,
                p.to_node,
                COALESCE(nt.short_name, '') AS to_short_name,
                COALESCE(nt.long_name, '') AS to_long_name,
                p.via_mqtt,
                p.hop_count,
                p.hop_start,
                p.rssi,
                p.snr
             FROM packets p
             LEFT JOIN nodes nf ON nf.node_id = p.from_node
             LEFT JOIN nodes nt ON nt.node_id = p.to_node
             WHERE p.direction = 'in'
               AND p.packet_type = 'traceroute'
               AND p.timestamp > ?1
               {mqtt_clause}
             ORDER BY p.timestamp DESC, p.id DESC
             LIMIT ?2"
        );

        let rows = conn
            .prepare(&query)?
            .query_map(params![since, limit as i64], |row| {
                let from_node_i64: i64 = row.get(1)?;
                let to_node_i64: Option<i64> = row.get(4)?;
                let via_mqtt_i64: i64 = row.get(7)?;
                let hop_count_i64: Option<i64> = row.get(8)?;
                let hop_start_i64: Option<i64> = row.get(9)?;
                Ok(TracerouteEvent {
                    timestamp: row.get(0)?,
                    from_node: format!("!{:08x}", from_node_i64 as u32),
                    from_short_name: row.get(2)?,
                    from_long_name: row.get(3)?,
                    to_node: to_node_i64
                        .map(|n| format!("!{:08x}", n as u32))
                        .unwrap_or_else(|| "broadcast".to_string()),
                    to_short_name: row.get(5)?,
                    to_long_name: row.get(6)?,
                    via_mqtt: via_mqtt_i64 != 0,
                    hop_count: hop_count_i64.map(|h| h as u32),
                    hop_start: hop_start_i64.map(|h| h as u32),
                    rssi: row.get(10)?,
                    snr: row.get(11)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn dashboard_traceroute_destinations(
        &self,
        hours: u32,
        filter: MqttFilter,
    ) -> Result<Vec<TracerouteDestinationSummary>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let since = if hours == 0 {
            0
        } else {
            Utc::now().timestamp() - (hours as i64 * 3600)
        };
        let mqtt_clause = match filter {
            MqttFilter::All => "",
            MqttFilter::LocalOnly => " AND p.via_mqtt = 0",
            MqttFilter::MqttOnly => " AND p.via_mqtt = 1",
        };

        let query = format!(
            "SELECT
                p.to_node,
                COALESCE(nt.short_name, '') AS to_short_name,
                COALESCE(nt.long_name, '') AS to_long_name,
                COUNT(*) AS requests,
                COUNT(DISTINCT p.from_node) AS unique_requesters,
                MAX(p.timestamp) AS last_seen,
                SUM(CASE WHEN p.via_mqtt = 0 THEN 1 ELSE 0 END) AS rf_count,
                SUM(CASE WHEN p.via_mqtt = 1 THEN 1 ELSE 0 END) AS mqtt_count,
                AVG(p.hop_count) AS avg_hops
             FROM packets p
             LEFT JOIN nodes nt ON nt.node_id = p.to_node
             WHERE p.direction = 'in'
               AND p.packet_type = 'traceroute'
               AND p.timestamp > ?1
               {mqtt_clause}
             GROUP BY p.to_node, nt.short_name, nt.long_name
             ORDER BY last_seen DESC"
        );

        let rows = conn
            .prepare(&query)?
            .query_map(params![since], |row| {
                let to_node_i64: Option<i64> = row.get(0)?;
                let requests: i64 = row.get(3)?;
                let unique_requesters: i64 = row.get(4)?;
                let rf_count: i64 = row.get(6)?;
                let mqtt_count: i64 = row.get(7)?;
                Ok(TracerouteDestinationSummary {
                    destination_node: to_node_i64
                        .map(|n| format!("!{:08x}", n as u32))
                        .unwrap_or_else(|| "broadcast".to_string()),
                    destination_short_name: row.get(1)?,
                    destination_long_name: row.get(2)?,
                    requests: requests as u64,
                    unique_requesters: unique_requesters as u64,
                    last_seen: row.get(5)?,
                    rf_count: rf_count as u64,
                    mqtt_count: mqtt_count as u64,
                    avg_hops: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    fn traceroute_status(
        request_hops: Option<u32>,
        response_hops: Option<u32>,
        request_route_len: usize,
        response_route_len: usize,
    ) -> &'static str {
        let req_present = request_hops.is_some() || request_route_len > 0;
        let res_present = response_hops.is_some() || response_route_len > 0;
        if req_present && res_present {
            "complete"
        } else if req_present || res_present {
            "partial"
        } else {
            "observed"
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn log_traceroute_observation(
        &self,
        packet_row_id: i64,
        trace_key: &str,
        src_node: u32,
        dst_node: Option<u32>,
        via_mqtt: bool,
        request_hops: Option<u32>,
        request_hop_start: Option<u32>,
        response_hops: Option<u32>,
        response_hop_start: Option<u32>,
        request_route: &[u32],
        response_route: &[u32],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let now = Utc::now().timestamp();
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        let session_id = {
            let mut find_stmt = tx.prepare(
                "SELECT id, first_seen, request_hops, request_hop_start, response_hops, response_hop_start, sample_count
                 FROM traceroute_sessions
                 WHERE trace_key = ?1
                 LIMIT 1",
            )?;
            let existing = find_stmt.query_row(params![trace_key], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            });

            match existing {
                Ok((
                    id,
                    first_seen,
                    req_hops_prev,
                    req_start_prev,
                    res_hops_prev,
                    res_start_prev,
                    sample_count,
                )) => {
                    let merged_req_hops = request_hops.or(req_hops_prev.map(|v| v as u32));
                    let merged_req_start = request_hop_start.or(req_start_prev.map(|v| v as u32));
                    let merged_res_hops = response_hops.or(res_hops_prev.map(|v| v as u32));
                    let merged_res_start = response_hop_start.or(res_start_prev.map(|v| v as u32));
                    let status = Self::traceroute_status(
                        merged_req_hops,
                        merged_res_hops,
                        request_route.len(),
                        response_route.len(),
                    );
                    tx.execute(
                        "UPDATE traceroute_sessions
                         SET first_seen = ?2,
                             last_seen = ?3,
                             src_node = ?4,
                             dst_node = ?5,
                             via_mqtt = ?6,
                             request_hops = ?7,
                             request_hop_start = ?8,
                             response_hops = ?9,
                             response_hop_start = ?10,
                             request_packet_id = CASE WHEN ?7 IS NOT NULL THEN COALESCE(request_packet_id, ?11) ELSE request_packet_id END,
                             response_packet_id = CASE WHEN ?9 IS NOT NULL THEN COALESCE(response_packet_id, ?11) ELSE response_packet_id END,
                             status = ?12,
                             sample_count = ?13
                         WHERE id = ?1",
                        params![
                            id,
                            std::cmp::min(first_seen, now),
                            now,
                            src_node as i64,
                            dst_node.map(|n| n as i64),
                            via_mqtt as i64,
                            merged_req_hops.map(|v| v as i64),
                            merged_req_start.map(|v| v as i64),
                            merged_res_hops.map(|v| v as i64),
                            merged_res_start.map(|v| v as i64),
                            packet_row_id,
                            status,
                            sample_count + 1,
                        ],
                    )?;
                    id
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => {
                    let status = Self::traceroute_status(
                        request_hops,
                        response_hops,
                        request_route.len(),
                        response_route.len(),
                    );
                    tx.execute(
                        "INSERT INTO traceroute_sessions
                         (trace_key, first_seen, last_seen, src_node, dst_node, via_mqtt, request_hops, request_hop_start, response_hops, response_hop_start, request_packet_id, response_packet_id, status, sample_count)
                         VALUES (?1, ?2, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 1)",
                        params![
                            trace_key,
                            now,
                            src_node as i64,
                            dst_node.map(|n| n as i64),
                            via_mqtt as i64,
                            request_hops.map(|v| v as i64),
                            request_hop_start.map(|v| v as i64),
                            response_hops.map(|v| v as i64),
                            response_hop_start.map(|v| v as i64),
                            if request_hops.is_some() {
                                Some(packet_row_id)
                            } else {
                                None
                            },
                            if response_hops.is_some() {
                                Some(packet_row_id)
                            } else {
                                None
                            },
                            status,
                        ],
                    )?;
                    tx.last_insert_rowid()
                }
                Err(e) => return Err(e.into()),
            }
        };

        for (idx, node) in request_route.iter().enumerate() {
            tx.execute(
                "INSERT INTO traceroute_session_hops (session_id, direction, hop_index, node_id, observed_at, packet_id_ref, source_kind)
                 VALUES (?1, 'request', ?2, ?3, ?4, ?5, 'route')",
                params![session_id, idx as i64, *node as i64, now, packet_row_id],
            )?;
        }
        for (idx, node) in response_route.iter().enumerate() {
            tx.execute(
                "INSERT INTO traceroute_session_hops (session_id, direction, hop_index, node_id, observed_at, packet_id_ref, source_kind)
                 VALUES (?1, 'response', ?2, ?3, ?4, ?5, 'route_back')",
                params![session_id, idx as i64, *node as i64, now, packet_row_id],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn dashboard_hops_to_me(
        &self,
        target_node: u32,
        hours: u32,
        filter: MqttFilter,
    ) -> Result<Vec<HopsToMeRow>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let since = if hours == 0 {
            0
        } else {
            Utc::now().timestamp() - (hours as i64 * 3600)
        };
        let mqtt_clause = match filter {
            MqttFilter::All => "",
            MqttFilter::LocalOnly => " AND p.via_mqtt = 0",
            MqttFilter::MqttOnly => " AND p.via_mqtt = 1",
        };

        let query = format!(
            "WITH filtered AS (
                SELECT p.*
                FROM packets p
                WHERE p.direction = 'in'
                  AND p.packet_type = 'traceroute'
                  AND p.to_node = ?1
                  AND p.timestamp > ?2
                  {mqtt_clause}
             ),
             latest_hops AS (
                SELECT from_node, hop_count,
                       ROW_NUMBER() OVER (PARTITION BY from_node ORDER BY timestamp DESC, id DESC) AS rn
                FROM filtered
                WHERE hop_count IS NOT NULL
             )
             SELECT
                f.from_node,
                COALESCE(n.short_name, '') AS short_name,
                COALESCE(n.long_name, '') AS long_name,
                COUNT(*) AS samples,
                MAX(f.timestamp) AS last_seen,
                lh.hop_count AS last_hops,
                MIN(f.hop_count) AS min_hops,
                AVG(f.hop_count) AS avg_hops,
                MAX(f.hop_count) AS max_hops,
                SUM(CASE WHEN f.via_mqtt = 0 THEN 1 ELSE 0 END) AS rf_count,
                SUM(CASE WHEN f.via_mqtt = 1 THEN 1 ELSE 0 END) AS mqtt_count
             FROM filtered f
             LEFT JOIN nodes n ON n.node_id = f.from_node
             LEFT JOIN latest_hops lh ON lh.from_node = f.from_node AND lh.rn = 1
             GROUP BY f.from_node, n.short_name, n.long_name, lh.hop_count
             ORDER BY last_seen DESC"
        );

        let rows = conn
            .prepare(&query)?
            .query_map(params![target_node as i64, since], |row| {
                let source_node_i64: i64 = row.get(0)?;
                let samples: i64 = row.get(3)?;
                let last_hops: Option<i64> = row.get(5)?;
                let min_hops: Option<i64> = row.get(6)?;
                let max_hops: Option<i64> = row.get(8)?;
                let rf_count: i64 = row.get(9)?;
                let mqtt_count: i64 = row.get(10)?;
                Ok(HopsToMeRow {
                    source_node: format!("!{:08x}", source_node_i64 as u32),
                    source_short_name: row.get(1)?,
                    source_long_name: row.get(2)?,
                    samples: samples as u64,
                    last_seen: row.get(4)?,
                    last_hops: last_hops.map(|h| h as u32),
                    min_hops: min_hops.map(|h| h as u32),
                    avg_hops: row.get(7)?,
                    max_hops: max_hops.map(|h| h as u32),
                    rf_count: rf_count as u64,
                    mqtt_count: mqtt_count as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn dashboard_traceroute_sessions(
        &self,
        hours: u32,
        filter: MqttFilter,
        limit: u32,
    ) -> Result<Vec<TracerouteSessionRow>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let since = if hours == 0 {
            0
        } else {
            Utc::now().timestamp() - (hours as i64 * 3600)
        };
        let mqtt_clause = match filter {
            MqttFilter::All => "",
            MqttFilter::LocalOnly => " AND s.via_mqtt = 0",
            MqttFilter::MqttOnly => " AND s.via_mqtt = 1",
        };

        let query = format!(
            "SELECT
                s.id,
                s.trace_key,
                s.src_node,
                COALESCE(ns.short_name, '') AS src_short_name,
                COALESCE(ns.long_name, '') AS src_long_name,
                s.dst_node,
                COALESCE(nd.short_name, '') AS dst_short_name,
                COALESCE(nd.long_name, '') AS dst_long_name,
                s.first_seen,
                s.last_seen,
                s.via_mqtt,
                s.request_hops,
                s.request_hop_start,
                s.response_hops,
                s.response_hop_start,
                s.status,
                s.sample_count
             FROM traceroute_sessions s
             LEFT JOIN nodes ns ON ns.node_id = s.src_node
             LEFT JOIN nodes nd ON nd.node_id = s.dst_node
             WHERE s.last_seen > ?1
               {mqtt_clause}
             ORDER BY s.last_seen DESC, s.id DESC
             LIMIT ?2"
        );

        let rows = conn
            .prepare(&query)?
            .query_map(params![since, limit as i64], |row| {
                let src_node_i64: i64 = row.get(2)?;
                let dst_node_i64: Option<i64> = row.get(5)?;
                let via_mqtt_i64: i64 = row.get(10)?;
                let request_hops: Option<i64> = row.get(11)?;
                let request_hop_start: Option<i64> = row.get(12)?;
                let response_hops: Option<i64> = row.get(13)?;
                let response_hop_start: Option<i64> = row.get(14)?;
                let sample_count: i64 = row.get(16)?;
                Ok(TracerouteSessionRow {
                    id: row.get(0)?,
                    trace_key: row.get(1)?,
                    src_node: format!("!{:08x}", src_node_i64 as u32),
                    src_short_name: row.get(3)?,
                    src_long_name: row.get(4)?,
                    dst_node: dst_node_i64
                        .map(|n| format!("!{:08x}", n as u32))
                        .unwrap_or_else(|| "broadcast".to_string()),
                    dst_short_name: row.get(6)?,
                    dst_long_name: row.get(7)?,
                    first_seen: row.get(8)?,
                    last_seen: row.get(9)?,
                    via_mqtt: via_mqtt_i64 != 0,
                    request_hops: request_hops.map(|v| v as u32),
                    request_hop_start: request_hop_start.map(|v| v as u32),
                    response_hops: response_hops.map(|v| v as u32),
                    response_hop_start: response_hop_start.map(|v| v as u32),
                    status: row.get(15)?,
                    sample_count: sample_count as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn dashboard_traceroute_session_detail(
        &self,
        session_id: i64,
    ) -> Result<Option<TracerouteSessionDetail>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let session: Result<TracerouteSessionRow, _> = conn.query_row(
            "SELECT
                s.id,
                s.trace_key,
                s.src_node,
                COALESCE(ns.short_name, '') AS src_short_name,
                COALESCE(ns.long_name, '') AS src_long_name,
                s.dst_node,
                COALESCE(nd.short_name, '') AS dst_short_name,
                COALESCE(nd.long_name, '') AS dst_long_name,
                s.first_seen,
                s.last_seen,
                s.via_mqtt,
                s.request_hops,
                s.request_hop_start,
                s.response_hops,
                s.response_hop_start,
                s.status,
                s.sample_count
             FROM traceroute_sessions s
             LEFT JOIN nodes ns ON ns.node_id = s.src_node
             LEFT JOIN nodes nd ON nd.node_id = s.dst_node
             WHERE s.id = ?1",
            params![session_id],
            |row| {
                let src_node_i64: i64 = row.get(2)?;
                let dst_node_i64: Option<i64> = row.get(5)?;
                let via_mqtt_i64: i64 = row.get(10)?;
                let request_hops: Option<i64> = row.get(11)?;
                let request_hop_start: Option<i64> = row.get(12)?;
                let response_hops: Option<i64> = row.get(13)?;
                let response_hop_start: Option<i64> = row.get(14)?;
                let sample_count: i64 = row.get(16)?;
                Ok(TracerouteSessionRow {
                    id: row.get(0)?,
                    trace_key: row.get(1)?,
                    src_node: format!("!{:08x}", src_node_i64 as u32),
                    src_short_name: row.get(3)?,
                    src_long_name: row.get(4)?,
                    dst_node: dst_node_i64
                        .map(|n| format!("!{:08x}", n as u32))
                        .unwrap_or_else(|| "broadcast".to_string()),
                    dst_short_name: row.get(6)?,
                    dst_long_name: row.get(7)?,
                    first_seen: row.get(8)?,
                    last_seen: row.get(9)?,
                    via_mqtt: via_mqtt_i64 != 0,
                    request_hops: request_hops.map(|v| v as u32),
                    request_hop_start: request_hop_start.map(|v| v as u32),
                    response_hops: response_hops.map(|v| v as u32),
                    response_hop_start: response_hop_start.map(|v| v as u32),
                    status: row.get(15)?,
                    sample_count: sample_count as u64,
                })
            },
        );

        let session = match session {
            Ok(s) => s,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        let hops = conn
            .prepare(
                "SELECT direction, hop_index, node_id, observed_at, source_kind
                 FROM traceroute_session_hops
                 WHERE session_id = ?1
                 ORDER BY CASE direction WHEN 'request' THEN 0 WHEN 'response' THEN 1 ELSE 2 END, hop_index ASC, id ASC",
            )?
            .query_map(params![session_id], |row| {
                let node_id_i64: i64 = row.get(2)?;
                let hop_index_i64: i64 = row.get(1)?;
                Ok(TracerouteSessionHop {
                    direction: row.get(0)?,
                    hop_index: hop_index_i64 as u32,
                    node_id: format!("!{:08x}", node_id_i64 as u32),
                    observed_at: row.get(3)?,
                    source_kind: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Some(TracerouteSessionDetail { session, hops }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Db {
        Db::open(Path::new(":memory:")).unwrap()
    }

    // --- Node tests ---

    #[test]
    fn test_upsert_and_get_node() {
        let db = setup_db();

        db.upsert_node(0x12345678, "ABCD", "Alice's Node", false)
            .unwrap();

        let nodes = db.get_all_nodes().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].node_id, 0x12345678);
        assert_eq!(nodes[0].short_name, "ABCD");
        assert_eq!(nodes[0].long_name, "Alice's Node");
    }

    #[test]
    fn test_is_node_new() {
        let db = setup_db();

        assert!(db.is_node_new(0x12345678).unwrap());

        db.upsert_node(0x12345678, "ABCD", "Alice", false).unwrap();

        assert!(!db.is_node_new(0x12345678).unwrap());
    }

    #[test]
    fn test_get_node_name_long() {
        let db = setup_db();
        db.upsert_node(0x12345678, "ABCD", "Alice's Node", false)
            .unwrap();

        let name = db.get_node_name(0x12345678).unwrap();
        assert_eq!(name, "Alice's Node");
    }

    #[test]
    fn test_get_node_name_short_fallback() {
        let db = setup_db();
        db.upsert_node(0x12345678, "ABCD", "", false).unwrap();

        let name = db.get_node_name(0x12345678).unwrap();
        assert_eq!(name, "ABCD");
    }

    #[test]
    fn test_get_node_name_hex_fallback() {
        let db = setup_db();
        db.upsert_node(0x12345678, "", "", false).unwrap();

        let name = db.get_node_name(0x12345678).unwrap();
        assert_eq!(name, "!12345678");
    }

    #[test]
    fn test_get_node_name_unknown() {
        let db = setup_db();
        let name = db.get_node_name(0x99999999).unwrap();
        assert_eq!(name, "!99999999");
    }

    #[test]
    fn test_purge_nodes_not_seen_within() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "A", "Alice", false).unwrap();
        db.upsert_node(0xBBBBBBBB, "B", "Bob", false).unwrap();

        let now = Utc::now().timestamp();
        let stale_ts = now - (8 * 24 * 3600);
        let recent_ts = now - (2 * 24 * 3600);
        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "UPDATE nodes SET last_seen = ?1 WHERE node_id = ?2",
                params![stale_ts, 0xAAAAAAAAu32 as i64],
            )
            .unwrap();
            conn.execute(
                "UPDATE nodes SET last_seen = ?1 WHERE node_id = ?2",
                params![recent_ts, 0xBBBBBBBBu32 as i64],
            )
            .unwrap();
        }

        let purged = db.purge_nodes_not_seen_within(7 * 24 * 3600).unwrap();
        assert_eq!(purged, 1);

        let nodes = db.get_all_nodes().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].node_id, 0xBBBBBBBB);
    }

    #[test]
    fn test_find_node_by_hex_id() {
        let db = setup_db();
        db.upsert_node(0xaabbccdd, "ABCD", "Alice", false).unwrap();

        assert_eq!(db.find_node_by_name("!aabbccdd").unwrap(), Some(0xaabbccdd));
        assert_eq!(db.find_node_by_name("aabbccdd").unwrap(), Some(0xaabbccdd));
    }

    #[test]
    fn test_find_node_by_decimal_id() {
        let db = setup_db();
        // Use a number with digits > 9 to avoid hex ambiguity
        db.upsert_node(3954221518, "ABCD", "Alice", false).unwrap();

        assert_eq!(
            db.find_node_by_name("3954221518").unwrap(),
            Some(3954221518)
        );
    }

    #[test]
    fn test_find_node_by_name() {
        let db = setup_db();
        db.upsert_node(0x12345678, "ABCD", "Alice", false).unwrap();

        assert_eq!(db.find_node_by_name("Alice").unwrap(), Some(0x12345678));
        assert_eq!(db.find_node_by_name("alice").unwrap(), Some(0x12345678)); // case insensitive
        assert_eq!(db.find_node_by_name("ABCD").unwrap(), Some(0x12345678));
    }

    #[test]
    fn test_find_node_not_found() {
        let db = setup_db();
        assert_eq!(db.find_node_by_name("Unknown").unwrap(), None);
    }

    #[test]
    fn test_get_recent_nodes_with_last_hop() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "A", "Alice", false).unwrap();
        db.upsert_node(0xBBBBBBBB, "B", "Bob", false).unwrap();

        db.log_packet(
            0xAAAAAAAA,
            None,
            0,
            "a1",
            "in",
            false,
            Some(-80),
            Some(5.0),
            Some(2),
            Some(7),
            "text",
        )
        .unwrap();
        db.log_packet(
            0xAAAAAAAA,
            None,
            0,
            "a2",
            "in",
            false,
            Some(-78),
            Some(5.2),
            Some(4),
            Some(7),
            "text",
        )
        .unwrap();

        let nodes = db.get_recent_nodes_with_last_hop(10).unwrap();
        assert_eq!(nodes.len(), 2);
        let limited = db.get_recent_nodes_with_last_hop(1).unwrap();
        assert_eq!(limited.len(), 1);

        let alice = nodes.iter().find(|n| n.node_id == 0xAAAAAAAA).unwrap();
        let bob = nodes.iter().find(|n| n.node_id == 0xBBBBBBBB).unwrap();
        assert_eq!(alice.last_hop, Some(4));
        assert_eq!(bob.last_hop, None);
    }

    #[test]
    fn test_recent_rf_node_missing_hops() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "A", "Alice", false).unwrap();
        db.upsert_node(0xBBBBBBBB, "B", "Bob", false).unwrap();

        // Bob already has hop metadata
        db.log_packet(
            0xBBBBBBBB,
            None,
            0,
            "hi",
            "in",
            false,
            Some(-80),
            Some(5.0),
            Some(2),
            Some(3),
            "text",
        )
        .unwrap();

        let candidate = db.recent_rf_node_missing_hops(3600, None).unwrap();
        assert_eq!(candidate, Some(0xAAAAAAAA));
    }

    #[test]
    fn test_recent_rf_node_missing_hops_excludes_node() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "A", "Alice", false).unwrap();
        db.upsert_node(0xBBBBBBBB, "B", "Bob", false).unwrap();

        let candidate = db
            .recent_rf_node_missing_hops(3600, Some(0xAAAAAAAA))
            .unwrap();
        assert_eq!(candidate, Some(0xBBBBBBBB));
    }

    #[test]
    fn test_recent_rf_nodes_missing_hops_returns_multiple_in_recency_order() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "A", "Alice", false).unwrap();
        db.upsert_node(0xBBBBBBBB, "B", "Bob", false).unwrap();
        db.upsert_node(0xCCCCCCCC, "C", "Carol", false).unwrap();

        let now = Utc::now().timestamp();
        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "UPDATE nodes SET last_seen = ?1 WHERE node_id = ?2",
                params![now - 30, 0xAAAAAAAAu32 as i64],
            )
            .unwrap();
            conn.execute(
                "UPDATE nodes SET last_seen = ?1 WHERE node_id = ?2",
                params![now - 10, 0xBBBBBBBBu32 as i64],
            )
            .unwrap();
            conn.execute(
                "UPDATE nodes SET last_seen = ?1 WHERE node_id = ?2",
                params![now - 20, 0xCCCCCCCCu32 as i64],
            )
            .unwrap();
        }

        let candidates = db.recent_rf_nodes_missing_hops(3600, None, 2).unwrap();
        assert_eq!(candidates, vec![0xBBBBBBBB, 0xCCCCCCCC]);
    }

    // --- Position tests ---

    #[test]
    fn test_update_and_get_position() {
        let db = setup_db();
        db.upsert_node(0x12345678, "ABCD", "Alice", false).unwrap();
        db.update_position(0x12345678, 25.0, 121.0).unwrap();

        let pos = db.get_node_position(0x12345678).unwrap();
        assert_eq!(pos, Some((25.0, 121.0)));
    }

    #[test]
    fn test_get_position_none() {
        let db = setup_db();
        db.upsert_node(0x12345678, "ABCD", "Alice", false).unwrap();

        let pos = db.get_node_position(0x12345678).unwrap();
        assert_eq!(pos, None);
    }

    #[test]
    fn test_get_position_zero_is_none() {
        let db = setup_db();
        db.upsert_node(0x12345678, "ABCD", "Alice", false).unwrap();
        db.update_position(0x12345678, 0.0, 0.0).unwrap();

        let pos = db.get_node_position(0x12345678).unwrap();
        assert_eq!(pos, None); // 0,0 is treated as no position
    }

    // --- Packet logging tests ---

    #[test]
    fn test_message_count() {
        let db = setup_db();

        assert_eq!(db.message_count("in").unwrap(), 0);
        assert_eq!(db.message_count("out").unwrap(), 0);

        db.log_packet(
            0x12345678,
            None,
            0,
            "Hello",
            "in",
            false,
            Some(-80),
            Some(5.0),
            Some(1),
            Some(3),
            "text",
        )
        .unwrap();
        db.log_packet(
            0x12345678,
            None,
            0,
            "World",
            "in",
            false,
            Some(-90),
            Some(3.0),
            Some(2),
            Some(3),
            "text",
        )
        .unwrap();
        db.log_packet(
            0x12345678,
            Some(0xaaaaaaaa),
            0,
            "Reply",
            "out",
            false,
            None,
            None,
            None,
            None,
            "text",
        )
        .unwrap();

        assert_eq!(db.message_count("in").unwrap(), 2);
        assert_eq!(db.message_count("out").unwrap(), 1);
    }

    #[test]
    fn test_node_count() {
        let db = setup_db();

        assert_eq!(db.node_count().unwrap(), 0);

        db.upsert_node(0xAAAAAAAA, "A", "Alice", false).unwrap();
        assert_eq!(db.node_count().unwrap(), 1);

        db.upsert_node(0xBBBBBBBB, "B", "Bob", false).unwrap();
        assert_eq!(db.node_count().unwrap(), 2);

        // Upsert same node doesn't increase count
        db.upsert_node(0xAAAAAAAA, "A", "Alice Updated", false)
            .unwrap();
        assert_eq!(db.node_count().unwrap(), 2);
    }

    // --- Upsert behavior tests ---

    #[test]
    fn test_upsert_updates_existing() {
        let db = setup_db();

        db.upsert_node(0x12345678, "OLD", "Old Name", false)
            .unwrap();
        db.upsert_node(0x12345678, "NEW", "New Name", false)
            .unwrap();

        let nodes = db.get_all_nodes().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].short_name, "NEW");
        assert_eq!(nodes[0].long_name, "New Name");
    }

    #[test]
    fn test_upsert_preserves_nonempty_names() {
        let db = setup_db();

        db.upsert_node(0x12345678, "ABCD", "Alice", false).unwrap();
        db.upsert_node(0x12345678, "", "", false).unwrap(); // Empty names shouldn't overwrite

        let nodes = db.get_all_nodes().unwrap();
        assert_eq!(nodes[0].short_name, "ABCD");
        assert_eq!(nodes[0].long_name, "Alice");
    }

    #[test]
    fn test_upsert_via_mqtt() {
        let db = setup_db();

        db.upsert_node(0x12345678, "ABCD", "Alice", false).unwrap();
        let nodes = db.dashboard_nodes(24, MqttFilter::All).unwrap();
        assert!(!nodes[0].via_mqtt);

        db.upsert_node(0x12345678, "ABCD", "Alice", true).unwrap();
        let nodes = db.dashboard_nodes(24, MqttFilter::All).unwrap();
        assert!(nodes[0].via_mqtt);
    }

    // --- Dashboard query tests ---

    #[test]
    fn test_dashboard_overview() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "A", "Alice", false).unwrap();
        db.upsert_node(0xBBBBBBBB, "B", "Bob", false).unwrap();
        db.log_packet(
            0xAAAAAAAA,
            None,
            0,
            "Hello",
            "in",
            false,
            Some(-80),
            Some(5.0),
            Some(1),
            Some(3),
            "text",
        )
        .unwrap();
        db.log_packet(
            0xBBBBBBBB,
            None,
            0,
            "Hi",
            "in",
            true,
            Some(-70),
            Some(8.0),
            Some(0),
            Some(3),
            "text",
        )
        .unwrap();
        db.log_packet(
            0xAAAAAAAA,
            Some(0xBBBBBBBB),
            0,
            "Reply",
            "out",
            false,
            None,
            None,
            None,
            None,
            "text",
        )
        .unwrap();
        // Non-text packet
        db.log_packet(
            0xAAAAAAAA,
            None,
            0,
            "",
            "in",
            false,
            Some(-75),
            Some(6.0),
            Some(1),
            Some(3),
            "position",
        )
        .unwrap();

        let overview = db
            .dashboard_overview(24, MqttFilter::All, "TestBot")
            .unwrap();
        assert_eq!(overview.node_count, 2);
        assert_eq!(overview.messages_in, 2);
        assert_eq!(overview.messages_out, 1);
        assert_eq!(overview.packets_in, 3); // 2 text + 1 position
        assert_eq!(overview.packets_out, 1);
        assert_eq!(overview.bot_name, "TestBot");

        let local = db
            .dashboard_overview(24, MqttFilter::LocalOnly, "TestBot")
            .unwrap();
        assert_eq!(local.messages_in, 1);

        let mqtt = db
            .dashboard_overview(24, MqttFilter::MqttOnly, "TestBot")
            .unwrap();
        assert_eq!(mqtt.messages_in, 1);
    }

    #[test]
    fn test_dashboard_traceroute_requesters() {
        let db = setup_db();
        let me = 0x01020304;
        let alice = 0xAAAAAAAA;
        let bob = 0xBBBBBBBB;

        db.upsert_node(alice, "ALC", "Alice", false).unwrap();
        db.upsert_node(bob, "BOB", "Bob", true).unwrap();

        db.log_packet(
            alice,
            Some(me),
            0,
            "",
            "in",
            false,
            Some(-90),
            Some(1.0),
            Some(1),
            Some(3),
            "traceroute",
        )
        .unwrap();
        db.log_packet(
            alice,
            Some(me),
            0,
            "",
            "in",
            false,
            Some(-88),
            Some(1.2),
            Some(1),
            Some(3),
            "traceroute",
        )
        .unwrap();
        db.log_packet(
            bob,
            Some(me),
            0,
            "",
            "in",
            true,
            Some(-70),
            Some(5.0),
            Some(0),
            Some(3),
            "traceroute",
        )
        .unwrap();
        db.log_packet(
            bob,
            Some(0x0A0B0C0D),
            0,
            "",
            "in",
            true,
            Some(-70),
            Some(5.0),
            Some(0),
            Some(3),
            "traceroute",
        )
        .unwrap();

        let all = db
            .dashboard_traceroute_requesters(me, 24, MqttFilter::All)
            .unwrap();
        assert_eq!(all.len(), 2);

        let alice_row = all.iter().find(|r| r.node_id == "!aaaaaaaa").unwrap();
        assert_eq!(alice_row.request_count, 2);
        assert_eq!(alice_row.long_name, "Alice");
        assert!(!alice_row.via_mqtt);

        let local_only = db
            .dashboard_traceroute_requesters(me, 24, MqttFilter::LocalOnly)
            .unwrap();
        assert_eq!(local_only.len(), 1);
        assert_eq!(local_only[0].node_id, "!aaaaaaaa");

        let mqtt_only = db
            .dashboard_traceroute_requesters(me, 24, MqttFilter::MqttOnly)
            .unwrap();
        assert_eq!(mqtt_only.len(), 1);
        assert_eq!(mqtt_only[0].node_id, "!bbbbbbbb");
    }

    #[test]
    fn test_dashboard_traceroute_events() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "ALC", "Alice", false).unwrap();
        db.upsert_node(0xBBBBBBBB, "BOB", "Bob", false).unwrap();

        db.log_packet(
            0xAAAAAAAA,
            Some(0xBBBBBBBB),
            0,
            "",
            "in",
            false,
            Some(-91),
            Some(1.5),
            Some(2),
            Some(3),
            "traceroute",
        )
        .unwrap();
        db.log_packet(
            0xAAAAAAAA,
            None,
            0,
            "",
            "in",
            true,
            Some(-70),
            Some(6.0),
            Some(0),
            Some(3),
            "traceroute",
        )
        .unwrap();

        let all = db
            .dashboard_traceroute_events(24, MqttFilter::All, 50)
            .unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].to_node, "broadcast");
        assert_eq!(all[1].to_node, "!bbbbbbbb");
        assert_eq!(all[1].from_long_name, "Alice");

        let local_only = db
            .dashboard_traceroute_events(24, MqttFilter::LocalOnly, 50)
            .unwrap();
        assert_eq!(local_only.len(), 1);
        assert!(!local_only[0].via_mqtt);
    }

    #[test]
    fn test_dashboard_traceroute_destinations() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "ALC", "Alice", false).unwrap();
        db.upsert_node(0xBBBBBBBB, "BOB", "Bob", false).unwrap();
        db.upsert_node(0xCCCCCCCC, "CAR", "Carol", false).unwrap();

        db.log_packet(
            0xAAAAAAAA,
            Some(0xBBBBBBBB),
            0,
            "",
            "in",
            false,
            Some(-90),
            Some(1.0),
            Some(1),
            Some(3),
            "traceroute",
        )
        .unwrap();
        db.log_packet(
            0xCCCCCCCC,
            Some(0xBBBBBBBB),
            0,
            "",
            "in",
            true,
            Some(-80),
            Some(2.0),
            Some(2),
            Some(3),
            "traceroute",
        )
        .unwrap();
        db.log_packet(
            0xAAAAAAAA,
            None,
            0,
            "",
            "in",
            false,
            Some(-85),
            Some(1.7),
            Some(0),
            Some(3),
            "traceroute",
        )
        .unwrap();

        let rows = db
            .dashboard_traceroute_destinations(24, MqttFilter::All)
            .unwrap();
        assert_eq!(rows.len(), 2);

        let bob = rows
            .iter()
            .find(|r| r.destination_node == "!bbbbbbbb")
            .unwrap();
        assert_eq!(bob.requests, 2);
        assert_eq!(bob.unique_requesters, 2);
        assert_eq!(bob.rf_count, 1);
        assert_eq!(bob.mqtt_count, 1);

        let broadcast = rows
            .iter()
            .find(|r| r.destination_node == "broadcast")
            .unwrap();
        assert_eq!(broadcast.requests, 1);
    }

    #[test]
    fn test_dashboard_hops_to_me() {
        let db = setup_db();
        let me = 0x01020304;
        let alice = 0xAAAAAAAA;
        let bob = 0xBBBBBBBB;
        db.upsert_node(alice, "ALC", "Alice", false).unwrap();
        db.upsert_node(bob, "BOB", "Bob", true).unwrap();

        db.log_packet(
            alice,
            Some(me),
            0,
            "",
            "in",
            false,
            Some(-90),
            Some(1.0),
            Some(2),
            Some(7),
            "traceroute",
        )
        .unwrap();
        db.log_packet(
            alice,
            Some(me),
            0,
            "",
            "in",
            false,
            Some(-88),
            Some(1.2),
            Some(1),
            Some(7),
            "traceroute",
        )
        .unwrap();
        db.log_packet(
            bob,
            Some(me),
            0,
            "",
            "in",
            true,
            Some(-70),
            Some(5.0),
            Some(3),
            Some(7),
            "traceroute",
        )
        .unwrap();

        let rows = db.dashboard_hops_to_me(me, 24, MqttFilter::All).unwrap();
        assert_eq!(rows.len(), 2);
        let alice_row = rows.iter().find(|r| r.source_node == "!aaaaaaaa").unwrap();
        assert_eq!(alice_row.samples, 2);
        assert_eq!(alice_row.last_hops, Some(1));
        assert_eq!(alice_row.min_hops, Some(1));
        assert_eq!(alice_row.max_hops, Some(2));
        assert_eq!(alice_row.rf_count, 2);
        assert_eq!(alice_row.mqtt_count, 0);
    }

    #[test]
    fn test_traceroute_sessions_and_detail() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "ALC", "Alice", false).unwrap();
        db.upsert_node(0xBBBBBBBB, "BOB", "Bob", false).unwrap();
        db.upsert_node(0xCCCCCCCC, "CAR", "Carol", false).unwrap();

        let packet_id = db
            .log_packet_with_mesh_id(
                0xAAAAAAAA,
                Some(0xBBBBBBBB),
                0,
                "",
                "in",
                false,
                Some(-90),
                Some(1.0),
                Some(2),
                Some(7),
                Some(0x11223344),
                "traceroute",
            )
            .unwrap();
        db.log_traceroute_observation(
            packet_id,
            "in:aaaaaaaa:bbbbbbbb:287454020",
            0xAAAAAAAA,
            Some(0xBBBBBBBB),
            false,
            Some(2),
            Some(7),
            Some(3),
            Some(7),
            &[0xAAAAAAAA, 0xCCCCCCCC, 0xBBBBBBBB],
            &[0xBBBBBBBB, 0xCCCCCCCC, 0xAAAAAAAA],
        )
        .unwrap();

        let sessions = db
            .dashboard_traceroute_sessions(24, MqttFilter::All, 50)
            .unwrap();
        assert_eq!(sessions.len(), 1);
        let session = &sessions[0];
        assert_eq!(session.status, "complete");
        assert_eq!(session.src_node, "!aaaaaaaa");
        assert_eq!(session.dst_node, "!bbbbbbbb");
        assert_eq!(session.request_hops, Some(2));

        let detail = db
            .dashboard_traceroute_session_detail(session.id)
            .unwrap()
            .unwrap();
        assert_eq!(detail.hops.len(), 6);
        assert_eq!(detail.hops[0].direction, "request");
        assert_eq!(detail.hops[0].node_id, "!aaaaaaaa");
    }

    #[test]
    fn test_dashboard_nodes() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "A", "Alice", false).unwrap();
        db.update_position(0xAAAAAAAA, 25.0, 121.0).unwrap();
        db.log_packet(
            0xAAAAAAAA,
            None,
            0,
            "Hello",
            "in",
            false,
            Some(-80),
            Some(5.0),
            Some(2),
            Some(3),
            "text",
        )
        .unwrap();
        db.log_packet(
            0xAAAAAAAA,
            None,
            0,
            "Again",
            "in",
            false,
            Some(-79),
            Some(5.2),
            Some(1),
            Some(3),
            "text",
        )
        .unwrap();

        let nodes = db.dashboard_nodes(24, MqttFilter::All).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].node_id, "!aaaaaaaa");
        assert_eq!(nodes[0].latitude, Some(25.0));
        assert!(!nodes[0].via_mqtt);
        assert_eq!(nodes[0].last_hop, Some(1));
        assert_eq!(nodes[0].min_hop, Some(1));
        assert_eq!(nodes[0].avg_hop, Some(1.5));
        assert_eq!(nodes[0].hop_samples, 2);
        assert!(nodes[0].last_rf_seen.is_some());
    }

    #[test]
    fn test_dashboard_nodes_mqtt_filter() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "A", "Alice", false).unwrap();
        db.upsert_node(0xBBBBBBBB, "B", "Bob", true).unwrap();

        let all = db.dashboard_nodes(24, MqttFilter::All).unwrap();
        assert_eq!(all.len(), 2);

        let local = db.dashboard_nodes(24, MqttFilter::LocalOnly).unwrap();
        assert_eq!(local.len(), 1);
        assert_eq!(local[0].node_id, "!aaaaaaaa");

        let mqtt = db.dashboard_nodes(24, MqttFilter::MqttOnly).unwrap();
        assert_eq!(mqtt.len(), 1);
        assert_eq!(mqtt[0].node_id, "!bbbbbbbb");
    }

    #[test]
    fn test_dashboard_nodes_hop_stats_respect_time_window() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "A", "Alice", false).unwrap();

        db.log_packet(
            0xAAAAAAAA,
            None,
            0,
            "old",
            "in",
            false,
            Some(-90),
            Some(2.0),
            Some(3),
            Some(3),
            "text",
        )
        .unwrap();
        db.log_packet(
            0xAAAAAAAA,
            None,
            0,
            "new",
            "in",
            false,
            Some(-80),
            Some(5.0),
            Some(1),
            Some(3),
            "text",
        )
        .unwrap();

        {
            let conn = db.conn.lock().unwrap();
            let old_ts = Utc::now().timestamp() - (48 * 3600);
            conn.execute(
                "UPDATE packets SET timestamp = ?1 WHERE text = 'old'",
                params![old_ts],
            )
            .unwrap();
        }

        let nodes_24h = db.dashboard_nodes(24, MqttFilter::All).unwrap();
        assert_eq!(nodes_24h.len(), 1);
        assert_eq!(nodes_24h[0].last_hop, Some(1));
        assert_eq!(nodes_24h[0].min_hop, Some(1));
        assert_eq!(nodes_24h[0].avg_hop, Some(1.0));
        assert_eq!(nodes_24h[0].hop_samples, 1);

        let nodes_all = db.dashboard_nodes(0, MqttFilter::All).unwrap();
        assert_eq!(nodes_all.len(), 1);
        assert_eq!(nodes_all[0].last_hop, Some(1));
        assert_eq!(nodes_all[0].min_hop, Some(1));
        assert_eq!(nodes_all[0].avg_hop, Some(2.0));
        assert_eq!(nodes_all[0].hop_samples, 2);
    }

    #[test]
    fn test_dashboard_throughput() {
        let db = setup_db();
        db.log_packet(
            0xAAAAAAAA,
            None,
            0,
            "Hello",
            "in",
            false,
            Some(-80),
            Some(5.0),
            Some(1),
            Some(3),
            "text",
        )
        .unwrap();
        db.log_packet(
            0xAAAAAAAA,
            Some(0xBBBBBBBB),
            0,
            "Reply",
            "out",
            false,
            None,
            None,
            None,
            None,
            "text",
        )
        .unwrap();
        // Non-text packets should not appear in text throughput
        db.log_packet(
            0xAAAAAAAA,
            None,
            0,
            "",
            "in",
            false,
            Some(-75),
            Some(6.0),
            Some(1),
            Some(3),
            "position",
        )
        .unwrap();

        let buckets = db.dashboard_throughput(24, MqttFilter::All).unwrap();
        assert!(!buckets.is_empty());
        let total_in: u64 = buckets.iter().map(|b| b.incoming).sum();
        let total_out: u64 = buckets.iter().map(|b| b.outgoing).sum();
        assert_eq!(total_in, 1);
        assert_eq!(total_out, 1);
    }

    #[test]
    fn test_dashboard_packet_throughput() {
        let db = setup_db();
        db.log_packet(
            0xAAAAAAAA,
            None,
            0,
            "Hello",
            "in",
            false,
            Some(-80),
            Some(5.0),
            Some(1),
            Some(3),
            "text",
        )
        .unwrap();
        db.log_packet(
            0xAAAAAAAA,
            None,
            0,
            "",
            "in",
            false,
            Some(-75),
            Some(6.0),
            Some(1),
            Some(3),
            "position",
        )
        .unwrap();
        db.log_packet(
            0xAAAAAAAA,
            None,
            0,
            "",
            "in",
            false,
            Some(-72),
            Some(7.0),
            Some(0),
            Some(3),
            "telemetry",
        )
        .unwrap();

        // All types
        let buckets = db
            .dashboard_packet_throughput(24, MqttFilter::All, None)
            .unwrap();
        let total_in: u64 = buckets.iter().map(|b| b.incoming).sum();
        assert_eq!(total_in, 3);

        // Filter to specific types
        let types = vec!["position".to_string(), "telemetry".to_string()];
        let buckets = db
            .dashboard_packet_throughput(24, MqttFilter::All, Some(&types))
            .unwrap();
        let total_in: u64 = buckets.iter().map(|b| b.incoming).sum();
        assert_eq!(total_in, 2);
    }

    #[test]
    fn test_dashboard_rssi() {
        let db = setup_db();
        db.log_packet(
            0xAAAAAAAA,
            None,
            0,
            "Hello",
            "in",
            false,
            Some(-80),
            Some(5.0),
            Some(1),
            Some(3),
            "text",
        )
        .unwrap();
        db.log_packet(
            0xAAAAAAAA,
            None,
            0,
            "World",
            "in",
            false,
            Some(-85),
            Some(3.0),
            Some(2),
            Some(3),
            "text",
        )
        .unwrap();

        let buckets = db.dashboard_rssi(24, MqttFilter::All).unwrap();
        assert!(!buckets.is_empty());
        let total: u64 = buckets.iter().map(|b| b.count).sum();
        assert_eq!(total, 2);
    }

    #[test]
    fn test_dashboard_hops() {
        let db = setup_db();
        db.log_packet(
            0xAAAAAAAA,
            None,
            0,
            "Hello",
            "in",
            false,
            Some(-80),
            Some(5.0),
            Some(1),
            Some(3),
            "text",
        )
        .unwrap();
        db.log_packet(
            0xAAAAAAAA,
            None,
            0,
            "World",
            "in",
            false,
            Some(-85),
            Some(3.0),
            Some(2),
            Some(3),
            "text",
        )
        .unwrap();

        let buckets = db.dashboard_hops(24, MqttFilter::All).unwrap();
        assert_eq!(buckets.len(), 2);
    }

    #[test]
    fn test_dashboard_positions() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "A", "Alice", false).unwrap();
        db.upsert_node(0xBBBBBBBB, "B", "Bob", false).unwrap();
        db.update_position(0xAAAAAAAA, 25.0, 121.0).unwrap();
        // Bob has no position

        let positions = db.dashboard_positions().unwrap();
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].node_id, "!aaaaaaaa");
    }

    #[test]
    fn test_log_packet_with_rf_metadata() {
        let db = setup_db();
        db.log_packet(
            0xAAAAAAAA,
            None,
            0,
            "Hello",
            "in",
            true,
            Some(-90),
            Some(5.5),
            Some(2),
            Some(3),
            "text",
        )
        .unwrap();

        // Verify it was stored by querying back
        let overview = db
            .dashboard_overview(24, MqttFilter::MqttOnly, "Test")
            .unwrap();
        assert_eq!(overview.messages_in, 1);

        let local = db
            .dashboard_overview(24, MqttFilter::LocalOnly, "Test")
            .unwrap();
        assert_eq!(local.messages_in, 0);
    }

    #[test]
    fn test_log_packet_types() {
        let db = setup_db();
        db.log_packet(
            0xAAAAAAAA,
            None,
            0,
            "Hello",
            "in",
            false,
            Some(-80),
            Some(5.0),
            Some(1),
            Some(3),
            "text",
        )
        .unwrap();
        db.log_packet(
            0xAAAAAAAA,
            None,
            0,
            "",
            "in",
            false,
            Some(-75),
            Some(6.0),
            Some(1),
            Some(3),
            "position",
        )
        .unwrap();
        db.log_packet(
            0xAAAAAAAA, None, 0, "", "in", false, None, None, None, None, "nodeinfo",
        )
        .unwrap();

        let overview = db.dashboard_overview(24, MqttFilter::All, "Test").unwrap();
        assert_eq!(overview.messages_in, 1); // Only text
        assert_eq!(overview.packets_in, 3); // All types
    }

    #[test]
    fn test_packet_throughput_rejects_invalid_types() {
        let db = setup_db();
        db.log_packet(
            0xAAAAAAAA,
            None,
            0,
            "Hello",
            "in",
            false,
            Some(-80),
            Some(5.0),
            Some(1),
            Some(3),
            "text",
        )
        .unwrap();

        // Invalid type names should be silently filtered out, returning empty
        let types = vec!["'; DROP TABLE packets; --".to_string()];
        let buckets = db
            .dashboard_packet_throughput(24, MqttFilter::All, Some(&types))
            .unwrap();
        assert!(buckets.is_empty());

        // Mix of valid and invalid — only valid types are used
        let types = vec!["text".to_string(), "fake_injection".to_string()];
        let buckets = db
            .dashboard_packet_throughput(24, MqttFilter::All, Some(&types))
            .unwrap();
        let total_in: u64 = buckets.iter().map(|b| b.incoming).sum();
        assert_eq!(total_in, 1);
    }
}
