use crate::{auth, config, db, paths};
use rand::{rngs::OsRng, RngCore};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<config::Config>,
    pub paths: Arc<paths::AppPaths>,
    pub token: Arc<String>,
    pub ui_session: Arc<String>,
    pub db: Arc<Mutex<db::Database>>,
    pub capture: Arc<Mutex<CaptureState>>,
}

#[derive(Debug)]
pub struct CaptureState {
    pub paused: bool,
    pub blocklist: config::BlocklistConfig,
    pub dedup_window_seconds: u32,
    pub prune: config::PruneConfig,
    pub suggestions: config::SuggestionsConfig,
}

impl AppState {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let paths = paths::AppPaths::resolve()?;
        paths.ensure()?;
        let config = config::Config::load_or_create(&paths)?;
        config.validate()?;
        let token = auth::load_or_create_token(&paths)?;
        let db = db::Database::open(&paths, &config)?;
        Ok(Self {
            capture: Arc::new(Mutex::new(CaptureState {
                paused: config.capture.paused,
                blocklist: config.capture.blocklist.clone(),
                dedup_window_seconds: config.capture.dedup_window_seconds,
                prune: config.prune.clone(),
                suggestions: config.suggestions.clone(),
            })),
            config: Arc::new(config),
            paths: Arc::new(paths),
            token: Arc::new(token),
            ui_session: Arc::new(random_session()),
            db: Arc::new(Mutex::new(db)),
        })
    }
}

fn random_session() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}
