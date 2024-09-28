
use crate::PlatformProvider;
use crate::PlatformInstaller;
use crate::path::get_home_relative_path;

const USER_SERVICE_TEMPLATE: &str = r#"
[Unit]
Description=Picture of the Day Wallpaper Changer
After=network.target
After=graphical-session.target

[Service]
Type=simple
ExecStart=%EXEC% run
Restart=on-failure

[Install]
WantedBy=graphical-session.target
"#;

pub struct GnuLinuxInstaller;

impl PlatformInstaller for GnuLinuxInstaller {
    fn install(&self) -> Result<(), std::io::Error> {
        let install_path = get_home_relative_path(".local/bin/potd-wallpaper");
        std::fs::create_dir_all(install_path.parent().unwrap())?;
        let current_exe = std::env::current_exe()?;
        std::fs::copy(&current_exe, &install_path)?;

        let service_path = get_home_relative_path(".local/lib/systemd/user/potd-wallpaper.service");
        std::fs::create_dir_all(service_path.parent().unwrap())?;
        let service_file = USER_SERVICE_TEMPLATE.replace("%EXEC%", &install_path.to_string_lossy());
        std::fs::write(&service_path, service_file)?;

        let mut command = std::process::Command::new("systemctl");
        command.arg("--user");
        command.arg("enable");
        command.arg("potd-wallpaper.service");
        command.output()?;

        let mut command = std::process::Command::new("systemctl");
        command.arg("--user");
        command.arg("restart");
        command.arg("potd-wallpaper.service");
        command.output()?;
        Ok(())
    }
}

pub struct GnuLinuxProvider;

impl PlatformProvider for GnuLinuxProvider {
    fn find_screen_resolution(&self) -> Result<(u32, u32), std::io::Error> {
        Ok((1920, 1080))
    }

    fn set_desktop_wallpaper_url(&self, url: &str) -> Result<(), std::io::Error> {
        let xdg_current_desktop = std::env::var("XDG_CURRENT_DESKTOP");
        if let Ok(xdg_current_desktop) = xdg_current_desktop {
            let xdg_current_desktop = xdg_current_desktop.to_lowercase();
            if xdg_current_desktop.contains("gnome") {
                let mut command = std::process::Command::new("gsettings");
                command.arg("set");
                command.arg("org.gnome.desktop.background");
                command.arg("picture-uri");
                command.arg(url);
                command.output()?;
            } else if xdg_current_desktop.contains("cinammon") {
                let mut command = std::process::Command::new("gsettings");
                command.arg("set");
                command.arg("org.cinnamon.desktop.background");
                command.arg("picture-uri");
                command.arg(url);
                command.output()?;
            } else {
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "Unsupported desktop environment"));
            }
        } else {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "XDG_CURRENT_DESKTOP is not set"));
        }
        Ok(())
    }
}
