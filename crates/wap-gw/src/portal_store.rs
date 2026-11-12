//! Persistent, cookie-keyed portal settings.
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    pub favorites: Vec<(String, String)>,
    pub weather: Option<(f64, f64, String)>,
}

pub struct Store {
    path: PathBuf,
}

fn error(e: impl std::fmt::Display) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, e.to_string())
}
fn valid_id(id: &str) -> bool {
    Uuid::parse_str(id).is_ok_and(|u| u.to_string() == id && u.get_version_num() == 4)
}

impl Store {
    pub fn new(path: impl AsRef<Path>) -> io::Result<Self> {
        fs::create_dir_all(path.as_ref())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path.as_ref(), fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self {
            path: path.as_ref().to_path_buf(),
        })
    }
    fn data_path(&self, id: &str) -> PathBuf {
        self.path.join(format!("{id}.json"))
    }
    fn locked(&self, id: &str) -> io::Result<File> {
        if !valid_id(id) {
            return Err(error("invalid session"));
        }
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.path.join(format!("{id}.lock")))?;
        f.lock()?;
        Ok(f)
    }
    fn read_locked(&self, id: &str) -> io::Result<Settings> {
        let bytes = fs::read(self.data_path(id))?;
        if bytes.len() > 1024 * 1024 {
            return Err(error("settings file too large"));
        }
        serde_json::from_slice(&bytes).map_err(error)
    }
    fn write_locked(&self, id: &str, data: &Settings) -> io::Result<()> {
        let bytes = serde_json::to_vec(data).map_err(error)?;
        if bytes.len() > 1024 * 1024 {
            return Err(error("settings file too large"));
        }
        let temp = self.path.join(format!(".{id}.{}.tmp", Uuid::new_v4()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temp, self.data_path(id))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temp);
        }
        result
    }
    pub fn session(&self, requested: Option<&str>) -> io::Result<(String, bool)> {
        if let Some(id) = requested.filter(|s| valid_id(s) && self.data_path(s).exists()) {
            let _lock = self.locked(id)?;
            if self.data_path(id).exists() {
                self.read_locked(id)?;
                return Ok((id.to_string(), false));
            }
        }
        let id = Uuid::new_v4().to_string();
        let _lock = self.locked(&id)?;
        self.write_locked(&id, &Settings::default())?;
        Ok((id, true))
    }
    pub fn read(&self, id: &str) -> io::Result<Settings> {
        let _lock = self.locked(id)?;
        self.read_locked(id)
    }
    pub fn update(&self, id: &str, f: impl FnOnce(&mut Settings)) -> io::Result<()> {
        let _lock = self.locked(id)?;
        let mut data = self.read_locked(id)?;
        f(&mut data);
        self.write_locked(id, &data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persists_and_keeps_sessions_separate() {
        let root = std::env::temp_dir().join(format!("portal-store-{}", Uuid::new_v4()));
        let store = Store::new(&root).unwrap();
        let (a, _) = store.session(None).unwrap();
        let (b, _) = store.session(None).unwrap();
        store
            .update(&a, |s| {
                s.weather = Some((40.0, -74.0, "New York".into()));
                s.favorites
                    .push(("Example".into(), "http://example.com".into()));
            })
            .unwrap();
        drop(store);
        let store = Store::new(&root).unwrap();
        assert!(!store.session(Some(&a)).unwrap().1);
        assert_eq!(store.read(&a).unwrap().favorites.len(), 1);
        assert!(store.read(&b).unwrap().favorites.is_empty());
        assert!(store.read(&b).unwrap().weather.is_none());
        assert!(store.session(Some("../../etc/passwd")).unwrap().1);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn concurrent_instances_do_not_lose_updates() {
        let root = std::env::temp_dir().join(format!("portal-store-{}", Uuid::new_v4()));
        let store = Store::new(&root).unwrap();
        let id = store.session(None).unwrap().0;
        let threads: Vec<_> = (0..4)
            .map(|n| {
                let root = root.clone();
                let id = id.clone();
                std::thread::spawn(move || {
                    let store = Store::new(root).unwrap();
                    for i in 0..10 {
                        store
                            .update(&id, |settings| {
                                settings
                                    .favorites
                                    .push((format!("{n}-{i}"), "http://example.com".into()))
                            })
                            .unwrap();
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(store.read(&id).unwrap().favorites.len(), 40);
        fs::remove_dir_all(root).unwrap();
    }
}
