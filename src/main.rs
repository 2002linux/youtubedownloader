use std::process::Command;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // getting the prompt the user for a YouTube URL
    print!("Enter the YouTube video URL: ");
    io::stdout().flush()?; // Ensure the prompt is displayed
    let mut url = String::new();
    io::stdin().read_line(&mut url)?;
    let url = url.trim();

    // Validate the URL
    if !is_valid_youtube_url(url) {
        eprintln!("Error: Invalid URL. Please enter a valid YouTube link.");
        return Ok(());
    }

    // then adding the yt-dlp binary path and ffmpeg binary path
    let current_dir = env::current_dir()?; // Get the current working directory
    let yt_dlp_path = current_dir.join("yt-dlp.exe"); // Relative path to yt-dlp.exe
    let ffmpeg_path = current_dir.join("ffmpeg").join("ffmpeg.exe"); // Relative path to ffmpeg.exe

    // adding an error handling to Check if yt-dlp.exe exists
    if !yt_dlp_path.exists() {
        eprintln!("Error: yt-dlp.exe not found at {}", yt_dlp_path.display());
        return Ok(());
    }

    // Check if ffmpeg.exe exists
    if !ffmpeg_path.exists() {
        eprintln!("Error: ffmpeg.exe not found at {}", ffmpeg_path.display());
        return Ok(());
    }

    let output_path = Path::new("."); // Save in the current directory

    // then let's run yt-dlp with the ffmpeg binary
    println!("Downloading video from: {}", url);
    let output_template = format!("{}/%(title)s.%(ext)s", output_path.display());
    let status = Command::new(yt_dlp_path)
        .args(&[
            "-f", "bestvideo+bestaudio/best", // Best video + audio
            "-o", &output_template,           // Output template
            "--ffmpeg-location", ffmpeg_path.to_str().unwrap(), // Specify ffmpeg location
            url,                              // YouTube URL
        ])
        .status();

    // last is to Check the status and handle errors
    match status {
        Ok(status) if status.success() => {
            println!("Download complete! Saved to {}", output_path.display());
        }
        Ok(_) => {
            eprintln!("Error: yt-dlp encountered an issue while processing the download.");
        }
        Err(e) => {
            eprintln!("Error: Failed to execute yt-dlp. Details: {}", e);
        }
    }

    Ok(())
}

/// Validates a given YouTube URL.
/// Returns true if valid, otherwise false.
fn is_valid_youtube_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}
