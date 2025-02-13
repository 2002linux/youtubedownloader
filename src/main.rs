use anyhow::{Context, Result};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use log::{error, info, warn};
use regex::Regex;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT};
use serde_json::Value;
use std::env;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use url::Url;

// For extracting the downloaded ffmpeg zip archive.
use std::io::Cursor;
use zip::ZipArchive;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the yt-dlp binary.
    #[arg(long, value_name = "PATH", default_value = "yt-dlp.exe")]
    yt_dlp_path: PathBuf,

    /// Path to the ffmpeg binary.
    #[arg(long, value_name = "PATH", default_value = "ffmpeg/ffmpeg.exe")]
    ffmpeg_path: PathBuf,

    /// Output directory for downloaded videos.
    ///
    /// The default is now "downloaded_videos". If the folder does not exist it will be created.
    #[arg(long, value_name = "PATH", default_value = "downloaded_videos")]
    output: PathBuf,

    /// Automatically check for yt-dlp and ffmpeg updates on startup.
    #[arg(long)]
    update: bool,

    /// If provided, run in non-interactive mode and download these URLs.
    #[arg(name = "URLS", num_args = 0..)]
    urls: Vec<String>,

    /// Run in non-interactive mode (requires at least one URL).
    #[arg(long)]
    non_interactive: bool,

    /// Retry delay in seconds (default is 10).
    #[arg(long, default_value = "10")]
    retry_delay: u64,
}

/// Parses a version string assumed to be in the "YYYY.MM.DD" format.
fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let re = Regex::new(r"^\s*(\d{4})\.(\d{1,2})\.(\d{1,2})").ok()?;
    let caps = re.captures(s)?;
    let year = caps.get(1)?.as_str().parse::<u32>().ok()?;
    let month = caps.get(2)?.as_str().parse::<u32>().ok()?;
    let day = caps.get(3)?.as_str().parse::<u32>().ok()?;
    Some((year, month, day))
}

/// Checks for updates to yt-dlp by comparing the current version with the latest release on GitHub.
fn update_yt_dlp(yt_dlp_path: &Path) -> Result<()> {
    info!("Checking for yt-dlp updates...");
    let output = Command::new(yt_dlp_path)
        .arg("--version")
        .output()
        .with_context(|| format!("Failed to execute {:?} --version", yt_dlp_path))?;

    if !output.status.success() {
        warn!("Failed to retrieve current yt-dlp version.");
        return Ok(());
    }
    let current_version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    info!("Current yt-dlp version: {}", current_version);

    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static("RustClient/1.0"));
    headers.insert(ACCEPT, HeaderValue::from_static("application/vnd.github.v3+json"));

    let client = Client::builder()
        .default_headers(headers)
        .build()
        .context("Failed to build HTTP client")?;
    let response = client
        .get("https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest")
        .send()
        .context("Failed to send request to GitHub API")?;
    if !response.status().is_success() {
        warn!(
            "Failed to fetch the latest yt-dlp version info. HTTP Status: {}",
            response.status()
        );
        return Ok(());
    }
    let json: Value = response.json().context("Failed to parse JSON from GitHub API")?;
    let latest_version = json["tag_name"].as_str().unwrap_or("").trim().to_string();
    if latest_version.is_empty() {
        warn!("Could not parse the latest version info.");
        return Ok(());
    }
    info!("Latest yt-dlp version: {}", latest_version);

    let need_update = if let (Some(current_parsed), Some(latest_parsed)) =
        (parse_version(&current_version), parse_version(&latest_version))
    {
        current_parsed < latest_parsed
    } else {
        current_version != latest_version
    };

    if need_update {
        info!("A newer yt-dlp version is available. Updating yt-dlp...");
        let status = Command::new(yt_dlp_path)
            .arg("-U")
            .status()
            .with_context(|| format!("Failed to execute {:?} -U", yt_dlp_path))?;
        if status.success() {
            info!("yt-dlp updated successfully.");
        } else {
            error!("yt-dlp update failed.");
        }
    } else {
        info!("The current yt-dlp is up-to-date.");
    }
    Ok(())
}

fn update_ffmpeg(ffmpeg_path: &Path) -> Result<()> {
    info!("Checking for ffmpeg updates...");

    let output = Command::new(ffmpeg_path)
        .arg("-version")
        .output()
        .with_context(|| format!("Failed to execute {:?} -version", ffmpeg_path))?;

    if !output.status.success() {
        warn!("Failed to retrieve current ffmpeg version.");
        return Ok(());
    }
    let current_version_output = String::from_utf8_lossy(&output.stdout);
    let current_version_line = current_version_output.lines().next().unwrap_or("");
    let current_version = current_version_line
        .split_whitespace()
        .nth(2)
        .unwrap_or("")
        .to_string();
    info!("Current ffmpeg version: {}", current_version);

    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static("RustClient/1.0"));
    headers.insert(ACCEPT, HeaderValue::from_static("application/vnd.github.v3+json"));
    let client = Client::builder()
        .default_headers(headers)
        .build()
        .context("Failed to build HTTP client for ffmpeg update")?;

    let response = client
        .get("https://api.github.com/repos/BtbN/FFmpeg-Builds/releases/latest")
        .send()
        .context("Failed to send request to GitHub API for ffmpeg")?;
    if !response.status().is_success() {
        warn!(
            "Failed to fetch the latest ffmpeg version info. HTTP Status: {}",
            response.status()
        );
        return Ok(());
    }
    let json: Value =
        response.json().context("Failed to parse JSON from GitHub API for ffmpeg")?;
    let tag_name = json["tag_name"].as_str().unwrap_or("").trim().to_string();
    if tag_name.is_empty() {
        warn!("Could not parse the latest ffmpeg version info.");
        return Ok(());
    }
    info!("Latest ffmpeg version: {}", tag_name);

    if current_version == tag_name {
        info!("The current ffmpeg is up-to-date.");
        return Ok(());
    }

    info!("A newer ffmpeg version is available. Updating ffmpeg...");

    let assets = json["assets"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("No assets found in ffmpeg release JSON"))?;
    let mut download_url = None;
    for asset in assets {
        if let Some(name) = asset["name"].as_str() {
            if name.to_lowercase().contains("win64") && name.to_lowercase().ends_with(".zip") {
                download_url = asset["browser_download_url"].as_str().map(|s| s.to_string());
                break;
            }
        }
    }
    let download_url = match download_url {
        Some(url) => url,
        None => {
            warn!("Could not find a suitable ffmpeg update asset for Windows 64-bit.");
            return Ok(());
        }
    };

    info!("Downloading ffmpeg update from {}", download_url);
    let resp = client
        .get(&download_url)
        .send()
        .context("Failed to download ffmpeg update")?;
    if !resp.status().is_success() {
        error!(
            "Failed to download ffmpeg update. HTTP Status: {}",
            resp.status()
        );
        return Ok(());
    }

    let bytes = resp
        .bytes()
        .context("Failed to read ffmpeg update response bytes")?;
    let reader = Cursor::new(bytes);
    let mut zip_archive =
        ZipArchive::new(reader).context("Failed to open zip archive for ffmpeg update")?;

    let mut ffmpeg_data = None;
    for i in 0..zip_archive.len() {
        let mut file = zip_archive
            .by_index(i)
            .context("Failed to access file in zip archive")?;
        let name = file.name().to_string();
        if name.to_lowercase().ends_with("ffmpeg.exe") {
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)
                .context("Failed to read ffmpeg.exe from zip archive")?;
            ffmpeg_data = Some(buf);
            break;
        }
    }
    let ffmpeg_data = match ffmpeg_data {
        Some(data) => data,
        None => {
            warn!("ffmpeg.exe not found in the downloaded archive.");
            return Ok(());
        }
    };

    std::fs::write(ffmpeg_path, ffmpeg_data)
        .with_context(|| format!("Failed to write updated ffmpeg to {:?}", ffmpeg_path))?;
    info!("ffmpeg updated successfully.");
    Ok(())
}

/// Validates a URL.
fn is_valid_url(url: &str) -> bool {
    Url::parse(url).is_ok()
}

/// Helper function to prompt the user (used only in interactive mode).
fn prompt_user(prompt: &str) -> Result<String> {
    print!("{}", prompt);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

/// Executes yt-dlp to download a video from the given URL.
/// It uses the resume flag (`-c`) and forces the output format to MP4.
fn download_video(
    yt_dlp_path: &Path,
    ffmpeg_path: &Path,
    output: &Path,
    url: &str,
) -> Result<()> {
    let output_template = format!("{}/%(title)s.%(ext)s", output.display());
    info!("Downloading video from: {}", url);

    let user_agent =
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:91.0) Gecko/20100101 Firefox/91.0";
    let headers = vec![
        (
            "Accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
        ),
        ("Accept-Language", "en-US,en;q=0.5"),
        ("Accept-Encoding", "gzip, deflate, br"),
        ("Connection", "keep-alive"),
        ("Upgrade-Insecure-Requests", "1"),
    ];

    let mut cmd = Command::new(yt_dlp_path);
    cmd.args(&[
        "-f",
        "bestvideo[height=720]+bestaudio/best[height=720]",
        "-c", // resume downloads
        "--merge-output-format",
        "mp4", // force MP4 output
        "-o",
        &output_template,
        "--ffmpeg-location",
        ffmpeg_path.to_str().unwrap(),
        "--user-agent",
        user_agent,
        "--newline",
    ]);
    for (key, value) in headers {
        cmd.args(&["--add-header", &format!("{}: {}", key, value)]);
    }
    cmd.arg(url);

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    info!("Running command: {:?}", cmd);

    let mut child = cmd.spawn().with_context(|| "Failed to spawn yt-dlp process")?;

    let pb = ProgressBar::new(100);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{bar:40.cyan/blue} {pos:>3}%")
            .unwrap()
            .progress_chars("##-"),
    );

    let stdout = child.stdout.take().expect("Failed to capture stdout");
    let stdout_thread = thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(line) = line {
                println!("{}", line);
            }
        }
    });

    let pb_clone = pb.clone();
    let stderr = child.stderr.take().expect("Failed to capture stderr");
    let stderr_thread = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        let progress_regex = Regex::new(r"\[download\]\s+(\d+\.\d+)%").unwrap();
        for line in reader.lines() {
            if let Ok(line) = line {
                if let Some(caps) = progress_regex.captures(&line) {
                    if let Some(percent_match) = caps.get(1) {
                        if let Ok(percent) = percent_match.as_str().parse::<f64>() {
                            pb_clone.set_position(percent.round() as u64);
                        }
                    }
                } else {
                    eprintln!("{}", line);
                }
            }
        }
    });

    let status = child.wait().with_context(|| "Failed to wait on yt-dlp process")?;
    pb.finish_with_message("Download complete!");

    stdout_thread.join().expect("Stdout thread panicked");
    stderr_thread.join().expect("Stderr thread panicked");

    if !status.success() {
        error!("yt-dlp failed with status: {}", status);
        return Err(anyhow::anyhow!("yt-dlp command failed with status {}", status));
    }

    info!("Download complete! Saved to {}", output.display());
    // At this point, the video file has been fully written, renamed (if needed),
    // and is no longer connected (i.e. locked or held open) by the downloader.
    info!("The downloaded video is now detached from the downloader.");
    Ok(())
}

fn download_video_robust(
    yt_dlp_path: &Path,
    ffmpeg_path: &Path,
    output: &Path,
    url: &str,
    retry_delay: u64,
) -> Result<()> {
    loop {
        match download_video(yt_dlp_path, ffmpeg_path, output, url) {
            Ok(_) => {
                info!("Download completed successfully.");
                break;
            }
            Err(e) => {
                error!(
                    "Download encountered an error: {:?}. Retrying in {} seconds...",
                    e, retry_delay
                );
                thread::sleep(Duration::from_secs(retry_delay));
                info!("Resuming download...");
            }
        }
    }
    Ok(())
}

/// Returns the directory of the current executable.
fn get_exe_dir() -> PathBuf {
    env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .expect("Could not determine the executable directory")
}

fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();
    let exe_dir = get_exe_dir();

    let yt_dlp_path = if args.yt_dlp_path.is_relative() {
        exe_dir.join(&args.yt_dlp_path)
    } else {
        args.yt_dlp_path.clone()
    };

    let ffmpeg_path = if args.ffmpeg_path.is_relative() {
        exe_dir.join(&args.ffmpeg_path)
    } else {
        args.ffmpeg_path.clone()
    };

    // Use the provided output directory.
    // Since we changed the default to "downloaded_videos", we now ensure it exists.
    let output = if args.output.is_relative() {
        exe_dir.join(&args.output)
    } else {
        args.output.clone()
    };
    if !output.exists() {
        std::fs::create_dir_all(&output)
            .with_context(|| format!("Failed to create output directory at {}", output.display()))?;
        info!("Created output directory at {}", output.display());
    }

    if !yt_dlp_path.exists() {
        error!("Error: yt-dlp not found at {}", yt_dlp_path.display());
        std::process::exit(1);
    }
    if !ffmpeg_path.exists() {
        error!("Error: ffmpeg not found at {}", ffmpeg_path.display());
        std::process::exit(1);
    }

    if args.update {
        update_yt_dlp(&yt_dlp_path)?;
        update_ffmpeg(&ffmpeg_path)?;
    }

    // Determine the mode: non-interactive (batch) or interactive.
    if args.non_interactive || !args.urls.is_empty() {
        if args.urls.is_empty() {
            error!("Non-interactive mode requires at least one URL.");
            std::process::exit(1);
        }
        for url in args.urls {
            if !is_valid_url(&url) {
                error!("Invalid URL: {}", url);
                continue;
            }
            download_video_robust(&yt_dlp_path, &ffmpeg_path, &output, &url, args.retry_delay)?;
        }
    } else {
        // Interactive mode.
        loop {
            let url = prompt_user("Enter the YouTube video URL (or type 'exit' to quit): ")?;
            if url.eq_ignore_ascii_case("exit") {
                break;
            }
            if !is_valid_url(&url) {
                error!("Error: Invalid URL. Please enter a valid YouTube link.");
                continue;
            }
            download_video_robust(&yt_dlp_path, &ffmpeg_path, &output, &url, args.retry_delay)?;
            let again = prompt_user("Do you want to download another video? (y/n): ")?;
            if !again.eq_ignore_ascii_case("y") {
                break;
            }
        }
    }
    Ok(())
}
