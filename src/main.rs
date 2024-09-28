
mod path;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
mod gnu_linux;

use std::sync::Arc;
use parking_lot::RwLock;
use rand::Rng;

use clap::Parser;
use clap::Subcommand;

#[derive(Debug, Parser)]
#[command(version, long_about = None)]
struct Args {
    #[command(subcommand)]
    subcmd: Command,
}

#[non_exhaustive]
#[derive(Debug, Subcommand)]
enum Command {
    /// Run the wallpaper changer service
    #[command()]
    Run {
        #[arg(short, long, default_value = "300")]
        change_interval: u64,
    },

    /// Install the wallpaper changer service for the current user
    #[command()]
    Install,
}

pub trait PlatformInstaller {
    fn install(&self) -> Result<(), std::io::Error>;
}

pub trait PlatformProvider {
    fn find_screen_resolution(&self) -> Result<(u32, u32), std::io::Error>;
    fn set_desktop_wallpaper_url(&self, url: &str) -> Result<(), std::io::Error>;
}

#[derive(Debug, Clone)]
pub struct Config {
    /// Wallpaper change interval in seconds
    pub wallpaper_interval: u64,

    /// Target screen resolution
    pub screen_resolution: (u32, u32),
}

impl Default for Config {
    fn default() -> Self {
        Self {
            wallpaper_interval: 60 * 10,
            screen_resolution: (1920, 1080),
        }
    }
}

#[derive(Clone)]
pub struct State {
    config: Arc<Config>,
    engine: Arc<potd::Engine>,
    http_client: potd::http_client::HttpClient,
    picture_urls: Arc<RwLock<Vec<String>>>,
}

impl State {
    pub fn new(config: Config) -> Self {
        let config = Arc::new(config);
        let engine = potd::Engine::new(config.screen_resolution.0.try_into().unwrap());
        let http_client = engine.fetcher().http_client();

        Self {
            config,
            engine: Arc::new(engine),
            http_client,
            picture_urls: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn engine(&self) -> &potd::Engine {
        &self.engine
    }

    pub fn http_client(&self) -> potd::http_client::HttpClient {
        self.http_client.clone()
    }

    pub fn set_urls(&self, urls: Vec<String>) {
        *self.picture_urls.write() = urls;
    }
}

#[allow(unused_variables)]
fn build_provider(state: &State) -> Box<dyn PlatformProvider> {
    #[cfg(target_os = "macos")]
    return Box::new(macos::MacosProvider::new(&state).unwrap());

    #[cfg(not(target_os = "macos"))]
    {
        #[cfg(target_os = "linux")]
        return Box::new(gnu_linux::GnuLinuxProvider);

        #[cfg(not(target_os = "linux"))]
        panic!("unsupported platform");
    }
}

fn build_installer() -> Box<dyn PlatformInstaller> {
    #[cfg(target_os = "macos")]
    return Box::new(macos::MacosInstaller);

    #[cfg(not(target_os = "macos"))]
    {
        #[cfg(target_os = "linux")]
        return Box::new(gnu_linux::GnuLinuxInstaller);

        #[cfg(not(target_os = "linux"))]
        panic!("unsupported platform");
    }
}

fn install() {
    let installer = build_installer();
    if let Err(e) = installer.install() {
        panic!("failed to install: {}", e);
    }
}

fn main() {
    env_logger::init();
    let args = Args::parse();
    let subcmd = args.subcmd;

    let change_interval = match subcmd {
        Command::Run { change_interval } => {
            change_interval
        }
        Command::Install => {
            install();
            return;
        }

        #[allow(unreachable_patterns)]
        _ => {
            log::error!("unsupported subcommand");
            return;
        },
    };

    let screen_resolution = {
        let config = Config::default();
        let state = State::new(config);

        let provider = build_provider(&state);

        provider.find_screen_resolution().unwrap_or((1920, 1080))
    };

    let mut config = Config::default();
    config.screen_resolution = screen_resolution;
    config.wallpaper_interval = change_interval;
    let state = State::new(config);
    let provider = build_provider(&state);

    let state_clone = state.clone();
    std::thread::spawn(move || {
        let state = state_clone;
        loop {
            let urls = if let Ok(urls) = state.engine().run_blocking() {
                urls
            } else {
                vec![]
            };
            if urls.is_empty() {
                std::thread::sleep(std::time::Duration::from_secs(60));
                continue;
            }

            state.set_urls(urls);
            std::thread::sleep(std::time::Duration::from_secs(60 * 60 * 6));
        }
    });

    let mut rng = rand::thread_rng();
    loop {
        let urls = loop {
            let urls = state.picture_urls.read().clone();
            if urls.is_empty() {
                std::thread::sleep(std::time::Duration::from_secs(5));
                continue;
            }
            break urls;
        };
        let index: u64 = rng.gen_range(0..urls.len() as u64);
        let url = &urls[index as usize];
        if let Err(e) = provider.set_desktop_wallpaper_url(url) {
            log::error!("failed to set wallpaper: {}", e);
        }
        std::thread::sleep(std::time::Duration::from_secs(state.config.wallpaper_interval));
    }
}
