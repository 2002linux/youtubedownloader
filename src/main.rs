use anyhow::{Context, Result};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use log::{error, info, warn};
use regex::Regex;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT};
use serde_json::Value;
use std::env;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use url::Url;

/// Simple CLI video downloader using yt-dlp and ffmpeg
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the yt-dlp binary
    #[arg(long, value_name = "PATH", default_value = "yt-dlp.exe")]
    yt_dlp_path: PathBuf,

    /// Path to the ffmpeg binary
    #[arg(long, value_name = "PATH", default_value = "ffmpeg/ffmpeg.exe")]
    ffmpeg_path: PathBuf,

    /// Output directory for downloaded videos
    #[arg(long, value_name = "PATH", default_value = ".")]
    output: PathBuf,

    /// Automatically check for yt-dlp updates on startup
    #[arg(long)]
    update: bool,
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
        warn!("Failed to fetch the latest yt-dlp version info. HTTP Status: {}", response.status());
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
        info!("A newer version is available. Updating yt-dlp...");
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

/// Validates a URL.
fn is_valid_url(url: &str) -> bool {
    Url::parse(url).is_ok()
}

/// Helper function to prompt the user.
fn prompt_user(prompt: &str) -> Result<String> {
    print!("{}", prompt);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

/// Executes yt-dlp to download a video from the given URL.
/// This version spawns the process and streams stdout and stderr in real time.
/// A progress bar is displayed by parsing progress messages from yt-dlp.
fn download_video(
    yt_dlp_path: &Path,
    ffmpeg_path: &Path,
    output: &Path,
    url: &str,
) -> Result<()> {
    let output_template = format!("{}/%(title)s.%(ext)s", output.display());
    info!("Downloading video from: {}", url);

    let user_agent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:91.0) Gecko/20100101 Firefox/91.0";
    let headers = vec![
        ("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8"),
        ("Accept-Language", "en-US,en;q=0.5"),
        ("Accept-Encoding", "gzip, deflate, br"),
        ("Connection", "keep-alive"),
        ("Upgrade-Insecure-Requests", "1"),
    ];

    // Build the command.
    // Updated -f flag to select only 720p video and its matching audio.
    let mut cmd = Command::new(yt_dlp_path);
    cmd.args(&[
        "-f", "bestvideo[height=720]+bestaudio/best[height=720]",
        "-o", &output_template,
        "--ffmpeg-location", ffmpeg_path.to_str().unwrap(),
        "--user-agent", user_agent,
        "--newline", // Ensure progress messages are printed on new lines
    ]);
    for (key, value) in headers {
        cmd.args(&["--add-header", &format!("{}: {}", key, value)]);
    }
    cmd.arg(url);

    // Set up the command to pipe stdout and stderr.
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    info!("Running command: {:?}", cmd);

    let mut child = cmd.spawn().with_context(|| "Failed to spawn yt-dlp process")?;

    // Create a progress bar with a range from 0 to 100.
    let pb = ProgressBar::new(100);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{bar:40.cyan/blue} {pos:>3}%")?
            .progress_chars("##-"),
    );

    // Spawn a thread to handle stdout.
    let stdout = child.stdout.take().expect("Failed to capture stdout");
    let stdout_thread = thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(line) = line {
                println!("{}", line);
            }
        }
    });

    // Spawn a thread to handle stderr and update the progress bar.
    let pb_clone = pb.clone();
    let stderr = child.stderr.take().expect("Failed to capture stderr");
    let stderr_thread = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        // Regex to capture progress percentage from lines like: "[download]  45.3%"
        let progress_regex = Regex::new(r"\[download\]\s+(\d+\.\d+)%").unwrap();
        for line in reader.lines() {
            if let Ok(line) = line {
                if let Some(caps) = progress_regex.captures(&line) {
                    if let Some(percent_match) = caps.get(1) {
                        if let Ok(percent) = percent_match.as_str().parse::<f64>() {
                            // Update progress bar (round to nearest integer)
                            pb_clone.set_position(percent.round() as u64);
                        }
                    }
                } else {
                    // If the line doesn't match progress info, print it to stderr.
                    eprintln!("{}", line);
                }
            }
        }
    });

    // Wait for the process to finish.
    let status = child.wait().with_context(|| "Failed to wait on yt-dlp process")?;
    pb.finish_with_message("Download complete!");

    // Ensure the output threads finish.
    stdout_thread.join().expect("Stdout thread panicked");
    stderr_thread.join().expect("Stderr thread panicked");

    if !status.success() {
        error!("yt-dlp failed with status: {}", status);
        return Err(anyhow::anyhow!("yt-dlp command failed with status {}", status));
    }
    info!("Download complete! Saved to {}", output.display());
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
    // Initialize logging.
    env_logger::init();

    // Parse command-line arguments.
    let args = Args::parse();

    // Determine the executable's directory.
    let exe_dir = get_exe_dir();

    // Resolve paths: if the provided path is relative, make it relative to the exe directory.
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

    let output = if args.output.is_relative() {
        exe_dir.join(&args.output)
    } else {
        args.output.clone()
    };

    if !yt_dlp_path.exists() {
        error!("Error: yt-dlp not found at {}", yt_dlp_path.display());
        return Ok(());
    }

    if !ffmpeg_path.exists() {
        error!("Error: ffmpeg not found at {}", ffmpeg_path.display());
        return Ok(());
    }

    if args.update {
        update_yt_dlp(&yt_dlp_path)?;
    } else {
        let answer = prompt_user("Do you want to check for yt-dlp updates? (y/n): ")?;
        if answer.eq_ignore_ascii_case("y") {
            update_yt_dlp(&yt_dlp_path)?;
        }
    }

    // Main download loop.
    loop {
        let url = prompt_user("Enter the YouTube video URL (or type 'exit' to quit): ")?;
        if url.eq_ignore_ascii_case("exit") {
            break;
        }
        if !is_valid_url(&url) {
            error!("Error: Invalid URL. Please enter a valid YouTube link.");
            continue;
        }
        if let Err(e) = download_video(&yt_dlp_path, &ffmpeg_path, &output, &url) {
            error!("Download failed: {:?}", e);
            let _ = prompt_user("Press Enter to continue...");
        }
        let again = prompt_user("Do you want to download another video? (y/n): ")?;
        if !again.eq_ignore_ascii_case("y") {
            break;
        }
    }
    Ok(())
}
