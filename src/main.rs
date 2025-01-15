use std::process::Command;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Prompt the user for a YouTube URL
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

    // Define paths to yt-dlp and ffmpeg binaries
    let current_dir = env::current_dir()?; // Get the current working directory
    let yt_dlp_path = current_dir.join("yt-dlp.exe"); // Relative path to yt-dlp.exe
    let ffmpeg_path = current_dir.join("ffmpeg").join("ffmpeg.exe"); // Relative path to ffmpeg.exe

    // Check if yt-dlp.exe exists
    if !yt_dlp_path.exists() {
        eprintln!("Error: yt-dlp.exe not found at {}", yt_dlp_path.display());
        return Ok(());
    }

    // Check if ffmpeg.exe exists
    if !ffmpeg_path.exists() {
        eprintln!("Error: ffmpeg.exe not found at {}", ffmpeg_path.display());
        return Ok(());
    }

    // Auto-update yt-dlp before proceeding
    println!("Checking for yt-dlp updates...");
    let update_status = Command::new(&yt_dlp_path)
        .arg("-U") // Update option for yt-dlp
        .status();

    match update_status {
        Ok(status) if status.success() => {
            println!("yt-dlp updated successfully.");
        }
        Ok(_) => {
            eprintln!("Warning: yt-dlp update check failed. Proceeding with the current version.");
        }
        Err(e) => {
            eprintln!("Error: Failed to update yt-dlp. Details: {}", e);
            return Ok(());
        }
    }

    // Define the output directory
    let output_path = Path::new("."); // Save in the current directory

    // Run yt-dlp with the ffmpeg binary
    println!("Downloading video from: {}", url);
    let output_template = format!("{}/%(title)s.%(ext)s", output_path.display());
    let status = Command::new(&yt_dlp_path)
        .args(&[
            "-f", "bestvideo+bestaudio/best", // Best video + audio
            "-o", &output_template,           // Output template
            "--ffmpeg-location", ffmpeg_path.to_str().unwrap(), // Specify ffmpeg location
            url,                              // YouTube URL
        ])
        .status();

    // Check the status and handle errors
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
