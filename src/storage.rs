use std::path::Path;

use anyhow::{Result, anyhow};
use rusqlite::Connection;

use crate::crawler::PageResult;

pub struct Storage {
    conn: Connection,
}

impl Storage {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        if conn.is_autocommit() {
            run_migration(&conn)?;
            Ok(Self { conn })
        } else {
            Err(anyhow!("Error connecting database"))
        }
    }
    pub fn close(self) -> Result<()> {
        self.conn.close().map_err(|(_, e)| e)?;
        Ok(())
    }
    pub fn save(&self, pr: PageResult) -> Result<()> {
        let sql = "INSERT INTO page_results (url, title, text_len, elapsed, status) VALUES (?1, ?2, ?3, ?4, ?5);";
        self.conn.execute(
            sql,
            (
                &pr.url,
                &pr.title,
                &(pr.text_len as i64),
                &(pr.elapsed as i64),
                &pr.status,
            ),
        )?;
        Ok(())
    }
}
fn run_migration(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- Создание таблицы
        CREATE TABLE IF NOT EXISTS page_results (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            url TEXT NOT NULL UNIQUE,
            title TEXT NOT NULL,
            text_len INTEGER NOT NULL,
            elapsed INTEGER NOT NULL,
            status INTEGER NOT NULL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );
        
        -- Индексы
        CREATE INDEX IF NOT EXISTS idx_page_results_url 
        ON page_results(url);
        
        CREATE INDEX IF NOT EXISTS idx_page_results_status 
        ON page_results(status);
        
        CREATE INDEX IF NOT EXISTS idx_page_results_created_at 
        ON page_results(created_at);
        "#,
    )?;

    Ok(())
}
