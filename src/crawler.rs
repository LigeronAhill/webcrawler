use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};

use anyhow::anyhow;
use reqwest::blocking::Client;
use scraper::{Html, Selector};
use tracing::{error, info};
use url::Url;

use crate::pool::ThreadPool;

pub struct Crawler {
    pool: Arc<ThreadPool>,
    visited: Arc<Mutex<HashSet<String>>>,
    active_jobs: Arc<AtomicUsize>,
    out_tx: std::sync::mpsc::Sender<PageResult>,
    client: Client,
    host: String,
    max_depth: u8,
}

impl Crawler {
    pub fn new(
        host: String,
        max_depth: u8,
        threads: usize,
        out_tx: std::sync::mpsc::Sender<PageResult>,
    ) -> anyhow::Result<Self> {
        let pool = ThreadPool::new(threads).ok_or(anyhow!("Failed to create thread pool"))?;

        let client = Client::builder()
            .user_agent("Ratacrawler/0.1")
            .timeout(std::time::Duration::from_secs(10))
            .build()?;

        Ok(Self {
            pool: Arc::new(pool),
            visited: Arc::new(Mutex::new(HashSet::new())),
            active_jobs: Arc::new(AtomicUsize::new(0)),
            out_tx,
            client,
            host,
            max_depth,
        })
    }

    pub fn start(&self, start_url: String) {
        self.active_jobs.fetch_add(1, Ordering::SeqCst);
        self.spawn_crawl_task(start_url, 0);
    }

    pub fn wait_for_completion(&self) {
        loop {
            let active = self.active_jobs.load(Ordering::SeqCst);
            if active == 0 {
                info!("All jobs completed");
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }

    fn spawn_crawl_task(&self, url: String, depth: u8) {
        let pool = self.pool.clone();
        let visited = self.visited.clone();
        let active_jobs = self.active_jobs.clone();
        let out_tx = self.out_tx.clone();
        let client = self.client.clone();
        let host = self.host.clone();
        let max_depth = self.max_depth;

        pool.spawn(move || {
            let result = Self::crawl_page(
                url,
                depth,
                max_depth,
                &host,
                visited,
                active_jobs.clone(),
                out_tx,
                client,
            );

            if let Err(e) = result {
                error!("Crawl error: {:?}", e);
            }

            // Уменьшаем счетчик после завершения задачи
            active_jobs.fetch_sub(1, Ordering::SeqCst);
        });
    }

    fn crawl_page(
        url: String,
        depth: u8,
        max_depth: u8,
        host: &str,
        visited: Arc<Mutex<HashSet<String>>>,
        active_jobs: Arc<AtomicUsize>,
        out_tx: std::sync::mpsc::Sender<PageResult>,
        client: Client,
    ) -> anyhow::Result<()> {
        // Проверяем глубину
        if depth > max_depth {
            info!("Depth {} > max {}, skipping {}", depth, max_depth, url);
            return Ok(());
        }

        // Проверяем посещенные URL
        {
            let mut v = visited.lock().unwrap();
            if !v.insert(url.clone()) {
                info!("Already visited: {}", url);
                return Ok(());
            }
        }

        info!("Fetching: {} (depth {})", url, depth);
        let started = Instant::now();

        let resp = match client.get(&url).send() {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to fetch {}: {:?}", url, e);
                return Ok(());
            }
        };

        let status = resp.status().as_u16();

        let body = match resp.text() {
            Ok(b) => b,
            Err(e) => {
                error!("Failed to get body from {}: {:?}", url, e);
                return Ok(());
            }
        };

        let elapsed = started.elapsed().as_millis();

        let (title, links, text_len) = match parse_page(&body, &url) {
            Ok(result) => result,
            Err(e) => {
                error!("Failed to parse page {}: {:?}", url, e);
                return Ok(());
            }
        };

        // Сохраняем результат
        let pr = PageResult {
            url: url.clone(),
            title,
            text_len,
            elapsed,
            status,
        };

        if let Err(e) = out_tx.send(pr) {
            error!("Failed to send result: {:?}", e);
        }

        // Обрабатываем ссылки
        if depth < max_depth {
            for link in links {
                if same_host(&link, host) && !link.contains('?') && !link.contains('#') {
                    active_jobs.fetch_add(1, Ordering::SeqCst);

                    // Клонируем все Arc для новой задачи
                    let visited_clone = visited.clone();
                    let active_clone = active_jobs.clone();
                    let out_clone = out_tx.clone();
                    let client_clone = client.clone();
                    let host_clone = host.to_string();

                    // Запускаем новую задачу в отдельном потоке
                    std::thread::spawn(move || {
                        let result = Self::crawl_page(
                            link,
                            depth + 1,
                            max_depth,
                            &host_clone,
                            visited_clone,
                            active_clone.clone(),
                            out_clone,
                            client_clone,
                        );
                        if let Err(e) = result {
                            error!("Error in spawned task: {:?}", e);
                        }
                        active_clone.fetch_sub(1, Ordering::SeqCst);
                    });
                }
            }
        }

        Ok(())
    }
}

fn parse_page(html: &str, base: &str) -> anyhow::Result<(String, Vec<String>, u64)> {
    let doc = Html::parse_document(html);
    let title_sel = Selector::parse("title").map_err(|e| anyhow!("{e:?}"))?;
    let a_sel = Selector::parse("a[href]").map_err(|e| anyhow!("{e:?}"))?;

    let title = doc
        .select(&title_sel)
        .next()
        .map(|t| t.text().collect::<String>())
        .unwrap_or_default()
        .trim()
        .to_string();

    let mut links = Vec::new();
    if let Ok(base_url) = Url::parse(base) {
        for a in doc.select(&a_sel) {
            if let Some(href) = a.value().attr("href") {
                if let Ok(joined) = base_url.join(href) {
                    let mut u = joined;
                    u.set_fragment(None);
                    if u.scheme() == "http" || u.scheme() == "https" {
                        links.push(u.to_string());
                    }
                }
            }
        }
    }

    let text_len: usize = doc.root_element().text().map(|s| s.len()).sum();
    Ok((title, links, text_len as u64))
}

fn same_host(url: &str, host: &str) -> bool {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .map(|h| {
            let h = h.trim_start_matches("www.");
            let host = host.trim_start_matches("www.");
            h == host
        })
        .unwrap_or(false)
}
pub struct PageResult {
    pub url: String,
    pub title: String,
    pub text_len: u64,
    pub elapsed: u128,
    pub status: u16,
}
