use chrono::Utc;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;

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
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub via_mqtt: bool,
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
pub struct MailMessage {
    pub id: i64,
    pub timestamp: i64,
    pub from_node: u32,
    pub body: String,
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
                packet_type TEXT NOT NULL DEFAULT 'text'
            );

            CREATE TABLE IF NOT EXISTS mail (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp  INTEGER NOT NULL,
                from_node  INTEGER NOT NULL,
                to_node    INTEGER NOT NULL,
                body       TEXT NOT NULL,
                read       INTEGER NOT NULL DEFAULT 0
            );",
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

    pub fn is_node_new(&self, node_id: u32) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
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

    pub fn mark_welcomed(&self, node_id: u32) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();
        conn.execute(
            "UPDATE nodes SET last_welcomed = ?1 WHERE node_id = ?2",
            params![now, node_id as i64],
        )?;
        Ok(())
    }

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

    pub fn get_node_name(&self, node_id: u32) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
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

    pub fn message_count(&self, direction: &str) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
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
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM nodes",
            [],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    pub fn find_node_by_name(&self, name: &str) -> Result<Option<u32>, Box<dyn std::error::Error + Send + Sync>> {
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

    pub fn store_mail(
        &self,
        from_node: u32,
        to_node: u32,
        body: &str,
    ) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();
        conn.execute(
            "INSERT INTO mail (timestamp, from_node, to_node, body) VALUES (?1, ?2, ?3, ?4)",
            params![now, from_node as i64, to_node as i64, body],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_unread_mail(&self, node_id: u32) -> Result<Vec<MailMessage>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, from_node, body FROM mail WHERE to_node = ?1 AND read = 0 ORDER BY timestamp ASC",
        )?;
        let mail = stmt
            .query_map(params![node_id as i64], |row| {
                Ok(MailMessage {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    from_node: row.get::<_, i64>(2)? as u32,
                    body: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(mail)
    }

    pub fn count_unread_mail(&self, node_id: u32) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM mail WHERE to_node = ?1 AND read = 0",
            params![node_id as i64],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    pub fn mark_mail_read(&self, mail_id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        conn.execute("UPDATE mail SET read = 1 WHERE id = ?1", params![mail_id])?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn mark_all_mail_read(&self, node_id: u32) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let count = conn.execute(
            "UPDATE mail SET read = 1 WHERE to_node = ?1 AND read = 0",
            params![node_id as i64],
        )?;
        Ok(count as u64)
    }

    pub fn delete_mail(&self, mail_id: i64, owner_node: u32) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let count = conn.execute(
            "DELETE FROM mail WHERE id = ?1 AND to_node = ?2",
            params![mail_id, owner_node as i64],
        )?;
        Ok(count > 0)
    }

    // --- Packet logging ---

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
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();
        conn.execute(
            "INSERT INTO packets (timestamp, from_node, to_node, channel, text, direction, via_mqtt, rssi, snr, hop_count, hop_start, packet_type)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
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
                packet_type,
            ],
        )?;
        Ok(())
    }

    // --- Dashboard queries ---

    pub fn dashboard_overview(&self, hours: u32, filter: MqttFilter, bot_name: &str) -> Result<DashboardOverview, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let node_count: i64 = conn.query_row("SELECT COUNT(*) FROM nodes", [], |row| row.get(0))?;
        let since = if hours == 0 { 0 } else { Utc::now().timestamp() - (hours as i64 * 3600) };

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

    pub fn dashboard_nodes(&self, filter: MqttFilter) -> Result<Vec<DashboardNode>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();

        let where_clause = match filter {
            MqttFilter::All => String::new(),
            MqttFilter::LocalOnly => " WHERE via_mqtt = 0".to_string(),
            MqttFilter::MqttOnly => " WHERE via_mqtt = 1".to_string(),
        };

        let query = format!(
            "SELECT node_id, short_name, long_name, last_seen, latitude, longitude, via_mqtt
             FROM nodes{} ORDER BY last_seen DESC",
            where_clause
        );
        let mut stmt = conn.prepare(&query)?;
        let nodes = stmt
            .query_map([], |row| {
                let nid: i64 = row.get(0)?;
                let via_mqtt_val: i64 = row.get(6)?;
                Ok(DashboardNode {
                    node_id: format!("!{:08x}", nid as u32),
                    short_name: row.get(1)?,
                    long_name: row.get(2)?,
                    last_seen: row.get(3)?,
                    latitude: row.get(4)?,
                    longitude: row.get(5)?,
                    via_mqtt: via_mqtt_val != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(nodes)
    }

    /// Throughput of text messages only (existing chart).
    pub fn dashboard_throughput(&self, hours: u32, filter: MqttFilter) -> Result<Vec<ThroughputBucket>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let since = if hours == 0 { 0 } else { Utc::now().timestamp() - (hours as i64 * 3600) };

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
        let since = if hours == 0 { 0 } else { Utc::now().timestamp() - (hours as i64 * 3600) };

        let bucket_expr = if hours > 48 {
            "strftime('%Y-%m-%d', timestamp, 'unixepoch')"
        } else {
            "strftime('%Y-%m-%d %H:00', timestamp, 'unixepoch')"
        };

        const VALID_PACKET_TYPES: &[&str] = &[
            "text", "position", "telemetry", "nodeinfo",
            "traceroute", "neighborinfo", "routing", "other",
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

    pub fn dashboard_rssi(&self, hours: u32, filter: MqttFilter) -> Result<Vec<DistributionBucket>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let since = if hours == 0 { 0 } else { Utc::now().timestamp() - (hours as i64 * 3600) };

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

    pub fn dashboard_snr(&self, hours: u32, filter: MqttFilter) -> Result<Vec<DistributionBucket>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let since = if hours == 0 { 0 } else { Utc::now().timestamp() - (hours as i64 * 3600) };

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

    pub fn dashboard_hops(&self, hours: u32, filter: MqttFilter) -> Result<Vec<DistributionBucket>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let since = if hours == 0 { 0 } else { Utc::now().timestamp() - (hours as i64 * 3600) };

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

    pub fn dashboard_positions(&self) -> Result<Vec<DashboardNode>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT node_id, short_name, long_name, last_seen, latitude, longitude, via_mqtt
             FROM nodes
             WHERE latitude IS NOT NULL AND longitude IS NOT NULL
               AND (latitude != 0.0 OR longitude != 0.0)
             ORDER BY last_seen DESC",
        )?;
        let nodes = stmt
            .query_map([], |row| {
                let nid: i64 = row.get(0)?;
                let via_mqtt_val: i64 = row.get(6)?;
                Ok(DashboardNode {
                    node_id: format!("!{:08x}", nid as u32),
                    short_name: row.get(1)?,
                    long_name: row.get(2)?,
                    last_seen: row.get(3)?,
                    latitude: row.get(4)?,
                    longitude: row.get(5)?,
                    via_mqtt: via_mqtt_val != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(nodes)
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

        db.upsert_node(0x12345678, "ABCD", "Alice's Node", false).unwrap();

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
        db.upsert_node(0x12345678, "ABCD", "Alice's Node", false).unwrap();

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

        assert_eq!(db.find_node_by_name("3954221518").unwrap(), Some(3954221518));
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

    // --- Mail tests ---

    #[test]
    fn test_store_and_get_mail() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "A", "Alice", false).unwrap();
        db.upsert_node(0xBBBBBBBB, "B", "Bob", false).unwrap();

        let id = db.store_mail(0xAAAAAAAA, 0xBBBBBBBB, "Hello Bob!").unwrap();
        assert!(id > 0);

        let mail = db.get_unread_mail(0xBBBBBBBB).unwrap();
        assert_eq!(mail.len(), 1);
        assert_eq!(mail[0].from_node, 0xAAAAAAAA);
        assert_eq!(mail[0].body, "Hello Bob!");
    }

    #[test]
    fn test_count_unread_mail() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "A", "Alice", false).unwrap();
        db.upsert_node(0xBBBBBBBB, "B", "Bob", false).unwrap();

        assert_eq!(db.count_unread_mail(0xBBBBBBBB).unwrap(), 0);

        db.store_mail(0xAAAAAAAA, 0xBBBBBBBB, "Message 1").unwrap();
        assert_eq!(db.count_unread_mail(0xBBBBBBBB).unwrap(), 1);

        db.store_mail(0xAAAAAAAA, 0xBBBBBBBB, "Message 2").unwrap();
        assert_eq!(db.count_unread_mail(0xBBBBBBBB).unwrap(), 2);
    }

    #[test]
    fn test_mark_mail_read() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "A", "Alice", false).unwrap();
        db.upsert_node(0xBBBBBBBB, "B", "Bob", false).unwrap();

        let id = db.store_mail(0xAAAAAAAA, 0xBBBBBBBB, "Hello").unwrap();
        assert_eq!(db.count_unread_mail(0xBBBBBBBB).unwrap(), 1);

        db.mark_mail_read(id).unwrap();
        assert_eq!(db.count_unread_mail(0xBBBBBBBB).unwrap(), 0);
    }

    #[test]
    fn test_delete_mail() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "A", "Alice", false).unwrap();
        db.upsert_node(0xBBBBBBBB, "B", "Bob", false).unwrap();

        let id = db.store_mail(0xAAAAAAAA, 0xBBBBBBBB, "Hello").unwrap();

        // Bob can delete
        assert!(db.delete_mail(id, 0xBBBBBBBB).unwrap());

        // Already deleted
        assert!(!db.delete_mail(id, 0xBBBBBBBB).unwrap());
    }

    #[test]
    fn test_delete_mail_wrong_owner() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "A", "Alice", false).unwrap();
        db.upsert_node(0xBBBBBBBB, "B", "Bob", false).unwrap();

        let id = db.store_mail(0xAAAAAAAA, 0xBBBBBBBB, "Hello").unwrap();

        // Alice cannot delete Bob's mail
        assert!(!db.delete_mail(id, 0xAAAAAAAA).unwrap());
    }

    // --- Packet logging tests ---

    #[test]
    fn test_message_count() {
        let db = setup_db();

        assert_eq!(db.message_count("in").unwrap(), 0);
        assert_eq!(db.message_count("out").unwrap(), 0);

        db.log_packet(0x12345678, None, 0, "Hello", "in", false, Some(-80), Some(5.0), Some(1), Some(3), "text").unwrap();
        db.log_packet(0x12345678, None, 0, "World", "in", false, Some(-90), Some(3.0), Some(2), Some(3), "text").unwrap();
        db.log_packet(0x12345678, Some(0xaaaaaaaa), 0, "Reply", "out", false, None, None, None, None, "text").unwrap();

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
        db.upsert_node(0xAAAAAAAA, "A", "Alice Updated", false).unwrap();
        assert_eq!(db.node_count().unwrap(), 2);
    }

    // --- Upsert behavior tests ---

    #[test]
    fn test_upsert_updates_existing() {
        let db = setup_db();

        db.upsert_node(0x12345678, "OLD", "Old Name", false).unwrap();
        db.upsert_node(0x12345678, "NEW", "New Name", false).unwrap();

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
        let nodes = db.dashboard_nodes(MqttFilter::All).unwrap();
        assert!(!nodes[0].via_mqtt);

        db.upsert_node(0x12345678, "ABCD", "Alice", true).unwrap();
        let nodes = db.dashboard_nodes(MqttFilter::All).unwrap();
        assert!(nodes[0].via_mqtt);
    }

    // --- Dashboard query tests ---

    #[test]
    fn test_dashboard_overview() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "A", "Alice", false).unwrap();
        db.upsert_node(0xBBBBBBBB, "B", "Bob", false).unwrap();
        db.log_packet(0xAAAAAAAA, None, 0, "Hello", "in", false, Some(-80), Some(5.0), Some(1), Some(3), "text").unwrap();
        db.log_packet(0xBBBBBBBB, None, 0, "Hi", "in", true, Some(-70), Some(8.0), Some(0), Some(3), "text").unwrap();
        db.log_packet(0xAAAAAAAA, Some(0xBBBBBBBB), 0, "Reply", "out", false, None, None, None, None, "text").unwrap();
        // Non-text packet
        db.log_packet(0xAAAAAAAA, None, 0, "", "in", false, Some(-75), Some(6.0), Some(1), Some(3), "position").unwrap();

        let overview = db.dashboard_overview(24, MqttFilter::All, "TestBot").unwrap();
        assert_eq!(overview.node_count, 2);
        assert_eq!(overview.messages_in, 2);
        assert_eq!(overview.messages_out, 1);
        assert_eq!(overview.packets_in, 3); // 2 text + 1 position
        assert_eq!(overview.packets_out, 1);
        assert_eq!(overview.bot_name, "TestBot");

        let local = db.dashboard_overview(24, MqttFilter::LocalOnly, "TestBot").unwrap();
        assert_eq!(local.messages_in, 1);

        let mqtt = db.dashboard_overview(24, MqttFilter::MqttOnly, "TestBot").unwrap();
        assert_eq!(mqtt.messages_in, 1);
    }

    #[test]
    fn test_dashboard_nodes() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "A", "Alice", false).unwrap();
        db.update_position(0xAAAAAAAA, 25.0, 121.0).unwrap();

        let nodes = db.dashboard_nodes(MqttFilter::All).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].node_id, "!aaaaaaaa");
        assert_eq!(nodes[0].latitude, Some(25.0));
        assert!(!nodes[0].via_mqtt);
    }

    #[test]
    fn test_dashboard_nodes_mqtt_filter() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "A", "Alice", false).unwrap();
        db.upsert_node(0xBBBBBBBB, "B", "Bob", true).unwrap();

        let all = db.dashboard_nodes(MqttFilter::All).unwrap();
        assert_eq!(all.len(), 2);

        let local = db.dashboard_nodes(MqttFilter::LocalOnly).unwrap();
        assert_eq!(local.len(), 1);
        assert_eq!(local[0].node_id, "!aaaaaaaa");

        let mqtt = db.dashboard_nodes(MqttFilter::MqttOnly).unwrap();
        assert_eq!(mqtt.len(), 1);
        assert_eq!(mqtt[0].node_id, "!bbbbbbbb");
    }

    #[test]
    fn test_dashboard_throughput() {
        let db = setup_db();
        db.log_packet(0xAAAAAAAA, None, 0, "Hello", "in", false, Some(-80), Some(5.0), Some(1), Some(3), "text").unwrap();
        db.log_packet(0xAAAAAAAA, Some(0xBBBBBBBB), 0, "Reply", "out", false, None, None, None, None, "text").unwrap();
        // Non-text packets should not appear in text throughput
        db.log_packet(0xAAAAAAAA, None, 0, "", "in", false, Some(-75), Some(6.0), Some(1), Some(3), "position").unwrap();

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
        db.log_packet(0xAAAAAAAA, None, 0, "Hello", "in", false, Some(-80), Some(5.0), Some(1), Some(3), "text").unwrap();
        db.log_packet(0xAAAAAAAA, None, 0, "", "in", false, Some(-75), Some(6.0), Some(1), Some(3), "position").unwrap();
        db.log_packet(0xAAAAAAAA, None, 0, "", "in", false, Some(-72), Some(7.0), Some(0), Some(3), "telemetry").unwrap();

        // All types
        let buckets = db.dashboard_packet_throughput(24, MqttFilter::All, None).unwrap();
        let total_in: u64 = buckets.iter().map(|b| b.incoming).sum();
        assert_eq!(total_in, 3);

        // Filter to specific types
        let types = vec!["position".to_string(), "telemetry".to_string()];
        let buckets = db.dashboard_packet_throughput(24, MqttFilter::All, Some(&types)).unwrap();
        let total_in: u64 = buckets.iter().map(|b| b.incoming).sum();
        assert_eq!(total_in, 2);
    }

    #[test]
    fn test_dashboard_rssi() {
        let db = setup_db();
        db.log_packet(0xAAAAAAAA, None, 0, "Hello", "in", false, Some(-80), Some(5.0), Some(1), Some(3), "text").unwrap();
        db.log_packet(0xAAAAAAAA, None, 0, "World", "in", false, Some(-85), Some(3.0), Some(2), Some(3), "text").unwrap();

        let buckets = db.dashboard_rssi(24, MqttFilter::All).unwrap();
        assert!(!buckets.is_empty());
        let total: u64 = buckets.iter().map(|b| b.count).sum();
        assert_eq!(total, 2);
    }

    #[test]
    fn test_dashboard_hops() {
        let db = setup_db();
        db.log_packet(0xAAAAAAAA, None, 0, "Hello", "in", false, Some(-80), Some(5.0), Some(1), Some(3), "text").unwrap();
        db.log_packet(0xAAAAAAAA, None, 0, "World", "in", false, Some(-85), Some(3.0), Some(2), Some(3), "text").unwrap();

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
        db.log_packet(0xAAAAAAAA, None, 0, "Hello", "in", true, Some(-90), Some(5.5), Some(2), Some(3), "text").unwrap();

        // Verify it was stored by querying back
        let overview = db.dashboard_overview(24, MqttFilter::MqttOnly, "Test").unwrap();
        assert_eq!(overview.messages_in, 1);

        let local = db.dashboard_overview(24, MqttFilter::LocalOnly, "Test").unwrap();
        assert_eq!(local.messages_in, 0);
    }

    #[test]
    fn test_log_packet_types() {
        let db = setup_db();
        db.log_packet(0xAAAAAAAA, None, 0, "Hello", "in", false, Some(-80), Some(5.0), Some(1), Some(3), "text").unwrap();
        db.log_packet(0xAAAAAAAA, None, 0, "", "in", false, Some(-75), Some(6.0), Some(1), Some(3), "position").unwrap();
        db.log_packet(0xAAAAAAAA, None, 0, "", "in", false, None, None, None, None, "nodeinfo").unwrap();

        let overview = db.dashboard_overview(24, MqttFilter::All, "Test").unwrap();
        assert_eq!(overview.messages_in, 1); // Only text
        assert_eq!(overview.packets_in, 3); // All types
    }

    #[test]
    fn test_packet_throughput_rejects_invalid_types() {
        let db = setup_db();
        db.log_packet(0xAAAAAAAA, None, 0, "Hello", "in", false, Some(-80), Some(5.0), Some(1), Some(3), "text").unwrap();

        // Invalid type names should be silently filtered out, returning empty
        let types = vec!["'; DROP TABLE packets; --".to_string()];
        let buckets = db.dashboard_packet_throughput(24, MqttFilter::All, Some(&types)).unwrap();
        assert!(buckets.is_empty());

        // Mix of valid and invalid — only valid types are used
        let types = vec!["text".to_string(), "fake_injection".to_string()];
        let buckets = db.dashboard_packet_throughput(24, MqttFilter::All, Some(&types)).unwrap();
        let total_in: u64 = buckets.iter().map(|b| b.incoming).sum();
        assert_eq!(total_in, 1);
    }
}
