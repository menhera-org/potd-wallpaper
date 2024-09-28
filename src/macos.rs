
use tokio;
use tokio::io::AsyncWriteExt;

use std::{path::PathBuf, sync::atomic::AtomicUsize};

use crate::path::get_home_relative_path;
use crate::PlatformProvider;

pub async fn run_osascript(script: &str) -> Result<String, std::io::Error> {
    let output = tokio::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output().await?;
    Ok(String::from_utf8(output.stdout).unwrap())
}

pub async fn find_screen_resolution() -> Result<(u32, u32), std::io::Error> {
    let output = run_osascript("tell application \"Finder\" to get bounds of window of desktop").await?;
    let parts = output.split(", ").collect::<Vec<_>>();
    if 4 != parts.len() {
        return Err(std::io::Error::new(std::io::ErrorKind::Other, "unexpected output"));
    }
    let x1: u32 = parts[0].trim().parse().unwrap();
    let y1: u32 = parts[1].trim().parse().unwrap();
    let x2: u32 = parts[2].trim().parse().unwrap();
    let y2: u32 = parts[3].trim().parse().unwrap();

    if x1 > x2 {
        return Err(std::io::Error::new(std::io::ErrorKind::Other, "unexpected output"));
    }

    if y1 > y2 {
        return Err(std::io::Error::new(std::io::ErrorKind::Other, "unexpected output"));
    }

    let x = x2 - x1;
    let y = y2 - y1;
    Ok((x, y))
}

pub async fn set_desktop_wallpaper(path: &str) -> Result<(), std::io::Error> {
    run_osascript(&format!("tell application \"System Events\" to tell every desktop to set picture to \"{}\" as POSIX file", path)).await?;
    Ok(())
}

async fn create_wallpaper_directory() -> Result<PathBuf, std::io::Error> {
    let path = get_home_relative_path("Library/potd-wallpaper");
    tokio::fs::create_dir_all(&path).await?;
    Ok(path)
}

const AGENT_PLIST_TEMPLATE: &str = r#"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>org.menhera.potd-wallpaper</string>
    <key>ProgramArguments</key>
    <array>
        <string>%EXEC%</string>
        <string>run</string>
    </array>
    <key>KeepAlive</key>
    <true/>
</dict>
</plist>
"#;

pub struct MacosInstaller;

impl crate::PlatformInstaller for MacosInstaller {
    fn install(&self) -> Result<(), std::io::Error> {
        let wallpaper_dir = get_home_relative_path("Library/potd-wallpaper");
        std::fs::create_dir_all(&wallpaper_dir)?;

        let bin_path = wallpaper_dir.join("potd-wallpaper");
        let current_exe = std::env::current_exe()?;
        std::fs::copy(&current_exe, &bin_path)?;

        let agent_dir = get_home_relative_path("Library/LaunchAgents");
        std::fs::create_dir_all(&agent_dir)?;
        let agent_file = agent_dir.join("org.menhera.potd-wallpaper.plist");

        let agent_plist = AGENT_PLIST_TEMPLATE.replace("%EXEC%", &bin_path.to_string_lossy());
        std::fs::write(&agent_file, agent_plist)?;

        std::process::Command::new("launchctl")
            .arg("load")
            .arg(&agent_file)
            .output()?;
        Ok(())
    }
}

pub struct MacosProvider {
    http_client: potd::http_client::HttpClient,
    wallpaper_counter: AtomicUsize,
    wallpaper_directory: PathBuf,
}

impl MacosProvider {
    pub fn new(state: &crate::State) -> Result<Self, std::io::Error> {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let wallpaper_directory = create_wallpaper_directory().await?;
            Ok(Self {
                http_client: state.http_client(),
                wallpaper_counter: AtomicUsize::new(0),
                wallpaper_directory,
            })
        })
    }

    fn next_wallpaper_path(&self) -> PathBuf {
        let counter = self.wallpaper_counter.fetch_xor(1, std::sync::atomic::Ordering::Relaxed);
        let path = self.wallpaper_directory.join(format!("wallpaper-{}.jpg", counter));
        path
    }

    fn prev_wallpaper_path(&self) -> PathBuf {
        let counter = self.wallpaper_counter.load(std::sync::atomic::Ordering::Relaxed);
        let path = self.wallpaper_directory.join(format!("wallpaper-{}.jpg", counter));
        path
    }
}

impl PlatformProvider for MacosProvider {
    fn find_screen_resolution(&self) -> Result<(u32, u32), std::io::Error> {
        tokio::runtime::Runtime::new().unwrap().block_on(find_screen_resolution())
    }

    fn set_desktop_wallpaper_url(&self, url: &str) -> Result<(), std::io::Error> {
        let mut path = self.next_wallpaper_path();
        if path.exists() {
            path = self.next_wallpaper_path();
        }
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let bytes = self.http_client.fetch_bytes(url, true).await.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
            let mut file = tokio::fs::File::create(&path).await?;
            file.write_all(&bytes).await?;
            set_desktop_wallpaper(&path.to_string_lossy()).await?;
            let prev_path = self.prev_wallpaper_path();
            let _ = tokio::fs::remove_file(prev_path).await;
            Ok(())
        })
    }
}
