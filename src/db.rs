use chrono::Utc;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;

use crate::util::parse_node_id;

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
                longitude      REAL
            );

            CREATE TABLE IF NOT EXISTS messages (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp  INTEGER NOT NULL,
                from_node  INTEGER NOT NULL,
                to_node    INTEGER,
                channel    INTEGER NOT NULL,
                text       TEXT NOT NULL,
                direction  TEXT NOT NULL
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

        // Migrate existing databases that lack position columns
        let has_lat: bool = conn
            .prepare("SELECT latitude FROM nodes LIMIT 0")
            .is_ok();
        if !has_lat {
            conn.execute_batch(
                "ALTER TABLE nodes ADD COLUMN latitude REAL;
                 ALTER TABLE nodes ADD COLUMN longitude REAL;",
            )?;
        }

        Ok(())
    }

    pub fn upsert_node(
        &self,
        node_id: u32,
        short_name: &str,
        long_name: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();
        conn.execute(
            "INSERT INTO nodes (node_id, short_name, long_name, first_seen, last_seen)
             VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT(node_id) DO UPDATE SET
                short_name = CASE WHEN ?2 != '' THEN ?2 ELSE short_name END,
                long_name  = CASE WHEN ?3 != '' THEN ?3 ELSE long_name END,
                last_seen  = ?4",
            params![node_id as i64, short_name, long_name, now],
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
            "SELECT COUNT(*) FROM messages WHERE direction = ?1",
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

    pub fn log_message(
        &self,
        from_node: u32,
        to_node: Option<u32>,
        channel: u32,
        text: &str,
        direction: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();
        conn.execute(
            "INSERT INTO messages (timestamp, from_node, to_node, channel, text, direction)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                now,
                from_node as i64,
                to_node.map(|n| n as i64),
                channel as i64,
                text,
                direction,
            ],
        )?;
        Ok(())
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

        db.upsert_node(0x12345678, "ABCD", "Alice's Node").unwrap();

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

        db.upsert_node(0x12345678, "ABCD", "Alice").unwrap();

        assert!(!db.is_node_new(0x12345678).unwrap());
    }

    #[test]
    fn test_get_node_name_long() {
        let db = setup_db();
        db.upsert_node(0x12345678, "ABCD", "Alice's Node").unwrap();

        let name = db.get_node_name(0x12345678).unwrap();
        assert_eq!(name, "Alice's Node");
    }

    #[test]
    fn test_get_node_name_short_fallback() {
        let db = setup_db();
        db.upsert_node(0x12345678, "ABCD", "").unwrap();

        let name = db.get_node_name(0x12345678).unwrap();
        assert_eq!(name, "ABCD");
    }

    #[test]
    fn test_get_node_name_hex_fallback() {
        let db = setup_db();
        db.upsert_node(0x12345678, "", "").unwrap();

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
        db.upsert_node(0xaabbccdd, "ABCD", "Alice").unwrap();

        assert_eq!(db.find_node_by_name("!aabbccdd").unwrap(), Some(0xaabbccdd));
        assert_eq!(db.find_node_by_name("aabbccdd").unwrap(), Some(0xaabbccdd));
    }

    #[test]
    fn test_find_node_by_decimal_id() {
        let db = setup_db();
        // Use a number with digits > 9 to avoid hex ambiguity
        db.upsert_node(3954221518, "ABCD", "Alice").unwrap();

        assert_eq!(db.find_node_by_name("3954221518").unwrap(), Some(3954221518));
    }

    #[test]
    fn test_find_node_by_name() {
        let db = setup_db();
        db.upsert_node(0x12345678, "ABCD", "Alice").unwrap();

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
        db.upsert_node(0x12345678, "ABCD", "Alice").unwrap();
        db.update_position(0x12345678, 25.0, 121.0).unwrap();

        let pos = db.get_node_position(0x12345678).unwrap();
        assert_eq!(pos, Some((25.0, 121.0)));
    }

    #[test]
    fn test_get_position_none() {
        let db = setup_db();
        db.upsert_node(0x12345678, "ABCD", "Alice").unwrap();

        let pos = db.get_node_position(0x12345678).unwrap();
        assert_eq!(pos, None);
    }

    #[test]
    fn test_get_position_zero_is_none() {
        let db = setup_db();
        db.upsert_node(0x12345678, "ABCD", "Alice").unwrap();
        db.update_position(0x12345678, 0.0, 0.0).unwrap();

        let pos = db.get_node_position(0x12345678).unwrap();
        assert_eq!(pos, None); // 0,0 is treated as no position
    }

    // --- Mail tests ---

    #[test]
    fn test_store_and_get_mail() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "A", "Alice").unwrap();
        db.upsert_node(0xBBBBBBBB, "B", "Bob").unwrap();

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
        db.upsert_node(0xAAAAAAAA, "A", "Alice").unwrap();
        db.upsert_node(0xBBBBBBBB, "B", "Bob").unwrap();

        assert_eq!(db.count_unread_mail(0xBBBBBBBB).unwrap(), 0);

        db.store_mail(0xAAAAAAAA, 0xBBBBBBBB, "Message 1").unwrap();
        assert_eq!(db.count_unread_mail(0xBBBBBBBB).unwrap(), 1);

        db.store_mail(0xAAAAAAAA, 0xBBBBBBBB, "Message 2").unwrap();
        assert_eq!(db.count_unread_mail(0xBBBBBBBB).unwrap(), 2);
    }

    #[test]
    fn test_mark_mail_read() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "A", "Alice").unwrap();
        db.upsert_node(0xBBBBBBBB, "B", "Bob").unwrap();

        let id = db.store_mail(0xAAAAAAAA, 0xBBBBBBBB, "Hello").unwrap();
        assert_eq!(db.count_unread_mail(0xBBBBBBBB).unwrap(), 1);

        db.mark_mail_read(id).unwrap();
        assert_eq!(db.count_unread_mail(0xBBBBBBBB).unwrap(), 0);
    }

    #[test]
    fn test_delete_mail() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "A", "Alice").unwrap();
        db.upsert_node(0xBBBBBBBB, "B", "Bob").unwrap();

        let id = db.store_mail(0xAAAAAAAA, 0xBBBBBBBB, "Hello").unwrap();

        // Bob can delete
        assert!(db.delete_mail(id, 0xBBBBBBBB).unwrap());

        // Already deleted
        assert!(!db.delete_mail(id, 0xBBBBBBBB).unwrap());
    }

    #[test]
    fn test_delete_mail_wrong_owner() {
        let db = setup_db();
        db.upsert_node(0xAAAAAAAA, "A", "Alice").unwrap();
        db.upsert_node(0xBBBBBBBB, "B", "Bob").unwrap();

        let id = db.store_mail(0xAAAAAAAA, 0xBBBBBBBB, "Hello").unwrap();

        // Alice cannot delete Bob's mail
        assert!(!db.delete_mail(id, 0xAAAAAAAA).unwrap());
    }

    // --- Message logging tests ---

    #[test]
    fn test_message_count() {
        let db = setup_db();

        assert_eq!(db.message_count("in").unwrap(), 0);
        assert_eq!(db.message_count("out").unwrap(), 0);

        db.log_message(0x12345678, None, 0, "Hello", "in").unwrap();
        db.log_message(0x12345678, None, 0, "World", "in").unwrap();
        db.log_message(0x12345678, Some(0xaaaaaaaa), 0, "Reply", "out").unwrap();

        assert_eq!(db.message_count("in").unwrap(), 2);
        assert_eq!(db.message_count("out").unwrap(), 1);
    }

    #[test]
    fn test_node_count() {
        let db = setup_db();

        assert_eq!(db.node_count().unwrap(), 0);

        db.upsert_node(0xAAAAAAAA, "A", "Alice").unwrap();
        assert_eq!(db.node_count().unwrap(), 1);

        db.upsert_node(0xBBBBBBBB, "B", "Bob").unwrap();
        assert_eq!(db.node_count().unwrap(), 2);

        // Upsert same node doesn't increase count
        db.upsert_node(0xAAAAAAAA, "A", "Alice Updated").unwrap();
        assert_eq!(db.node_count().unwrap(), 2);
    }

    // --- Upsert behavior tests ---

    #[test]
    fn test_upsert_updates_existing() {
        let db = setup_db();

        db.upsert_node(0x12345678, "OLD", "Old Name").unwrap();
        db.upsert_node(0x12345678, "NEW", "New Name").unwrap();

        let nodes = db.get_all_nodes().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].short_name, "NEW");
        assert_eq!(nodes[0].long_name, "New Name");
    }

    #[test]
    fn test_upsert_preserves_nonempty_names() {
        let db = setup_db();

        db.upsert_node(0x12345678, "ABCD", "Alice").unwrap();
        db.upsert_node(0x12345678, "", "").unwrap(); // Empty names shouldn't overwrite

        let nodes = db.get_all_nodes().unwrap();
        assert_eq!(nodes[0].short_name, "ABCD");
        assert_eq!(nodes[0].long_name, "Alice");
    }
}
