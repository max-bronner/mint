use std::sync::{Arc, Mutex};

use mint_pool::Background;
use mint_vfs::Vfs;

use super::{load_game_data, GameData, LoadRefusal};

type Loaded = Result<(GameData, String), LoadRefusal>;

static PENDING: Mutex<Option<Background<Loaded>>> = Mutex::new(None);

pub(crate) fn start(vfs: Arc<Vfs>) {
    let job = mint_pool::background(move || load_game_data(&vfs));
    if let Ok(mut pending) = PENDING.lock() {
        *pending = Some(job);
    }
}

pub(crate) fn take() -> Option<Loaded> {
    let job = PENDING.lock().ok()?.take()?;
    Some(job.wait())
}
