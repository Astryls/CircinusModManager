//! On-disk state in one SQLite file: the parsed-mod cache, small key/value user state
//! (groups, pins, settings) and cached Circinus weights.

use crate::Result;
use rusqlite::{params, Connection};
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

pub struct Cache {
    conn: Mutex<Connection>,
}

impl Cache {
    pub fn open(path: &Path) -> Result<Cache> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS mod_cache(uid TEXT PRIMARY KEY, stamp TEXT NOT NULL, json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS kv(key TEXT PRIMARY KEY, json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS weights(package_id TEXT PRIMARY KEY, json TEXT NOT NULL, fetched_at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS dds_files(uid TEXT NOT NULL, rel TEXT NOT NULL, json TEXT NOT NULL, PRIMARY KEY(uid, rel));",
        )?;
        Ok(Cache { conn: Mutex::new(conn) })
    }

    pub fn in_memory() -> Result<Cache> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS mod_cache(uid TEXT PRIMARY KEY, stamp TEXT NOT NULL, json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS kv(key TEXT PRIMARY KEY, json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS weights(package_id TEXT PRIMARY KEY, json TEXT NOT NULL, fetched_at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS dds_files(uid TEXT NOT NULL, rel TEXT NOT NULL, json TEXT NOT NULL, PRIMARY KEY(uid, rel));",
        )?;
        Ok(Cache { conn: Mutex::new(conn) })
    }

    /// Forget specific mods so the next scan re-reads them (their folder contents changed
    /// in a way the stamp cannot see, e.g. DDS files written next to the PNGs).
    pub fn forget_mod_entries(&self, uids: &[String]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for uid in uids {
            tx.execute("DELETE FROM mod_cache WHERE uid = ?1", params![uid])?;
        }
        tx.commit()?;
        Ok(())
    }

    // ---- DDS manifest: every texture Circinus converted, per mod ----

    /// uid → entries.
    pub fn dds_all<T: DeserializeOwned>(&self) -> Result<HashMap<String, Vec<T>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT uid, json FROM dds_files")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut map: HashMap<String, Vec<T>> = HashMap::new();
        for row in rows {
            let (uid, json) = row?;
            if let Ok(v) = serde_json::from_str::<T>(&json) {
                map.entry(uid).or_default().push(v);
            }
        }
        Ok(map)
    }

    pub fn dds_entries<T: DeserializeOwned>(&self, uid: &str) -> Result<Vec<T>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT json FROM dds_files WHERE uid = ?1")?;
        let rows = stmt.query_map(params![uid], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            if let Ok(v) = serde_json::from_str::<T>(&row?) {
                out.push(v);
            }
        }
        Ok(out)
    }

    /// (uid, rel, entry) upserts.
    pub fn dds_store<T: Serialize>(&self, entries: &[(String, String, T)]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare("INSERT OR REPLACE INTO dds_files(uid, rel, json) VALUES (?1, ?2, ?3)")?;
            for (uid, rel, e) in entries {
                stmt.execute(params![uid, rel, serde_json::to_string(e)?])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Remove specific (uid, rel) rows.
    pub fn dds_delete(&self, uid: &str, rels: &[String]) -> Result<()> {
        if rels.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for rel in rels {
            tx.execute("DELETE FROM dds_files WHERE uid = ?1 AND rel = ?2", params![uid, rel])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Forget every converted texture of a mod (after a revert).
    pub fn dds_delete_all(&self, uid: &str) -> Result<()> {
        self.conn.lock().unwrap().execute("DELETE FROM dds_files WHERE uid = ?1", params![uid])?;
        Ok(())
    }

    /// uid → (stamp, json)
    pub fn load_mod_entries(&self) -> Result<HashMap<String, (String, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT uid, stamp, json FROM mod_cache")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, (r.get::<_, String>(1)?, r.get::<_, String>(2)?))))?;
        let mut map = HashMap::new();
        for row in rows {
            let (k, v) = row?;
            map.insert(k, v);
        }
        Ok(map)
    }

    pub fn store_mod_entries(&self, entries: &[(String, String, String)]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare("INSERT OR REPLACE INTO mod_cache(uid, stamp, json) VALUES (?1, ?2, ?3)")?;
            for (uid, stamp, json) in entries {
                stmt.execute(params![uid, stamp, json])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Drop cache rows for folders that no longer exist.
    pub fn prune_mod_entries(&self, live_uids: &[&str]) -> Result<()> {
        let live: std::collections::HashSet<&str> = live_uids.iter().copied().collect();
        let mut conn = self.conn.lock().unwrap();
        let stale: Vec<String> = {
            let mut stmt = conn.prepare("SELECT uid FROM mod_cache")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.filter_map(|r| r.ok()).filter(|u| !live.contains(u.as_str())).collect()
        };
        if stale.is_empty() {
            return Ok(());
        }
        let tx = conn.transaction()?;
        for uid in stale {
            tx.execute("DELETE FROM mod_cache WHERE uid = ?1", params![uid])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn clear_mod_cache(&self) -> Result<()> {
        self.conn.lock().unwrap().execute("DELETE FROM mod_cache", [])?;
        Ok(())
    }

    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT json FROM kv WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        match rows.next()? {
            Some(row) => {
                let json: String = row.get(0)?;
                Ok(Some(serde_json::from_str(&json)?))
            }
            None => Ok(None),
        }
    }

    pub fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        let json = serde_json::to_string(value)?;
        self.conn.lock().unwrap().execute("INSERT OR REPLACE INTO kv(key, json) VALUES (?1, ?2)", params![key, json])?;
        Ok(())
    }

    pub fn weights_all<T: DeserializeOwned>(&self) -> Result<HashMap<String, (T, i64)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT package_id, json, fetched_at FROM weights")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?;
        let mut map = HashMap::new();
        for row in rows {
            let (k, json, at) = row?;
            if let Ok(v) = serde_json::from_str::<T>(&json) {
                map.insert(k, (v, at));
            }
        }
        Ok(map)
    }

    pub fn weights_store<T: Serialize>(&self, entries: &[(String, T)], fetched_at: i64) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare("INSERT OR REPLACE INTO weights(package_id, json, fetched_at) VALUES (?1, ?2, ?3)")?;
            for (id, v) in entries {
                stmt.execute(params![id, serde_json::to_string(v)?, fetched_at])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}
