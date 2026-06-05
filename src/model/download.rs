use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

pub struct ModelUrls {
    pub vad: &'static str,
    pub embedder: &'static str,
}

pub const URLS: ModelUrls = ModelUrls {
    vad: "https://github.com/mzdk100/voxudio/releases/download/model/voice_activity_detector.onnx",
    // WeSpeaker VoxBlink2 + VoxCeleb2 FT SimAMResNet34 (256-dim embeddings)
    embedder: "https://wenet.org.cn/downloads?models=wespeaker&version=voxblink2_samresnet34_ft.onnx",
};

/// Download ONNX model files to `models_dir` if they are not already present.
/// Calls `on_progress(fraction)` with values 0.0–1.0 across both files.
pub async fn ensure_models(
    models_dir: &Path,
    on_progress: impl Fn(f32) + Send + Sync + 'static,
) -> Result<()> {
    std::fs::create_dir_all(models_dir)?;

    let files = [
        ("voice_activity_detector.onnx", URLS.vad),
        ("voxblink2_samresnet34_ft.onnx", URLS.embedder),
    ];

    let total = files.len() as f32;
    for (i, (filename, url)) in files.iter().enumerate() {
        let dest = models_dir.join(filename);
        let base_fraction = i as f32 / total;
        download_if_missing(&dest, url, |f| {
            on_progress(base_fraction + f / total);
        })
        .await
        .with_context(|| format!("failed to download {filename}"))?;
    }

    on_progress(1.0);
    Ok(())
}

async fn download_if_missing(
    dest: &PathBuf,
    url: &str,
    on_progress: impl Fn(f32),
) -> Result<()> {
    if dest.exists() {
        let size = std::fs::metadata(dest)?.len();
        if size > 0 {
            info!("Model already present: {} ({} KB)", dest.display(), size / 1024);
            on_progress(1.0);
            return Ok(());
        }
    }

    info!("Downloading {} → {}", url, dest.display());

    const MAX_RETRIES: u32 = 3;
    let mut last_err = None;

    for attempt in 1..=MAX_RETRIES {
        match try_download(url, dest, &on_progress).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                warn!("Download attempt {attempt}/{MAX_RETRIES} failed: {e}");
                last_err = Some(e);
                if attempt < MAX_RETRIES {
                    tokio::time::sleep(std::time::Duration::from_secs(attempt as u64 * 2)).await;
                }
            }
        }
    }

    Err(last_err.unwrap())
}

async fn try_download(url: &str, dest: &Path, on_progress: impl Fn(f32)) -> Result<()> {
    use tokio::io::AsyncWriteExt;

    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .send()
        .await
        .context("HTTP request failed")?;

    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("HTTP {status} for {url}");
    }

    let total_bytes = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;

    let tmp = dest.with_extension("onnx.tmp");
    let mut file = tokio::fs::File::create(&tmp)
        .await
        .context("failed to create temp file")?;

    let mut stream = response.bytes_stream();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("stream error")?;
        file.write_all(&chunk).await.context("write error")?;
        downloaded += chunk.len() as u64;
        if total_bytes > 0 {
            on_progress(downloaded as f32 / total_bytes as f32);
        }
    }

    file.flush().await?;
    drop(file);

    tokio::fs::rename(&tmp, dest)
        .await
        .context("failed to rename temp file")?;

    let final_size = tokio::fs::metadata(dest).await?.len();
    info!(
        "Downloaded {} ({} KB)",
        dest.file_name().unwrap_or_default().to_string_lossy(),
        final_size / 1024
    );
    Ok(())
}
