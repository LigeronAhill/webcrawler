use std::sync::mpsc;
use tracing::{error, info};
use url::Url;
use webcrawler::{App, Crawler, PageResult, Storage};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    match ratatui::run(|terminal| App::default().run(terminal)) {
        Ok(Some(form)) => {
            let repo = Storage::new("./webcrawler.db")?;
            let workers = std::thread::available_parallelism()
                .map(|c| c.get())
                .unwrap_or(4);

            let max_depth = form.max_depth.value;
            let start_url = form.start_url.value;

            info!("Starting crawl of {} with depth {}", start_url, max_depth);

            let host = Url::parse(&start_url)?
                .host_str()
                .ok_or(anyhow::anyhow!("Wrong start URL"))?
                .to_string();

            let (out_tx, out_rx) = mpsc::channel::<PageResult>();

            // Запускаем writer thread
            let writer_handle = std::thread::spawn(move || {
                let mut count = 0;
                for pr in out_rx {
                    count += 1;
                    info!("Saved page #{}: {} ({}ms)", count, pr.url, pr.elapsed);
                    if let Err(e) = repo.save(pr) {
                        error!("Failed to save: {:?}", e);
                    }
                }
                info!("Total pages saved: {}", count);
            });

            // Создаем и запускаем краулер
            let crawler = Crawler::new(host, max_depth, workers, out_tx.clone())?;
            crawler.start(start_url);

            // Ждем завершения всех задач
            crawler.wait_for_completion();

            drop(crawler);

            // Даем время на завершение последних операций
            std::thread::sleep(std::time::Duration::from_secs(1));

            // Закрываем канал и ждем writer
            drop(out_tx);
            writer_handle.join().unwrap();

            info!("Crawling finished");
        }
        Ok(None) => info!("Canceled"),
        Err(err) => error!("UI error: {err}"),
    }
    Ok(())
}
