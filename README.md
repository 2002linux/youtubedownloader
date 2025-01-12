# YouTube Downloader

This project is a simple YouTube downloader implemented in Rust. It uses external binaries (`yt-dlp.exe` and `ffmpeg.exe`) for downloading and processing videos.

## Features

- Downloads YouTube videos using `yt-dlp`.
    
- Supports video conversion and processing via `ffmpeg`.
    

## Requirements

- Rust toolchain installed ([Install Rust](https://rustup.rs/))
    
- `yt-dlp` binary ([Download yt-dlp](https://github.com/yt-dlp/yt-dlp))
    
- `ffmpeg` binary ([Download FFmpeg](https://ffmpeg.org/))
    

## Installation

1. Clone this repository:
    
    ```
    git clone https://github.com/username/youtubedownloader.git
    cd youtubedownloader
    ```
    
2. Place the required binaries in the project directory: Ensure `yt-dlp.exe` and `ffmpeg.exe` are in the expected locations. Refer to the folder structure below:
    
    ```
    youtubedownloader/
    ├── yt-dlp.exe
    ├── ffmpeg/
    │   └── ffmpeg.exe
    ├── src/
    │   └── main.rs
    ├── Cargo.toml
    ```
    
3. Build the project:
    
    ```
    cargo build --release
    ```
    
4. Run the executable:
    
    ```
    ./target/release/youtubedownloader
    ```
    

## Usage

Run the application, and it will automatically locate the required binaries if placed correctly.

Example command in `main.rs`:

```
Command::new("./yt-dlp.exe")
    .arg("-h") // Example: List yt-dlp options
    .output()
    .expect("Failed to execute yt-dlp");
```

## Folder Structure

The project expects the following structure:

```
youtubedownloader/
├── yt-dlp.exe              # yt-dlp binary
├── ffmpeg/
│   └── ffmpeg.exe          # ffmpeg binary
├── src/
│   └── main.rs             # Main source code
├── Cargo.toml              # Rust project configuration
├── Cargo.lock
```

## Contributing

Pull requests are welcome. For significant changes, please open an issue first to discuss what you would like to change.
