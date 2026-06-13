//! Live audio: monitor Cinder's synth while flying (the recorder muxes the WAV instead).
//! `MusicPlugin` renders the synth on a background thread at startup (multi-core, see
//! `audio::synth_track`) and **holds the show clock** (`AudioGate`) until the track is ready, so
//! picture and music always start together at t=0 — the score's `@@` anchors stay sample-locked.
//! `MARTIN_MUSIC=<wav>` (what the bundle ships) skips the render → no wait at all.

use bevy::audio::{AudioPlayer, AudioSource, PlaybackSettings};
use bevy::prelude::*;

use crate::scene::SeqClock;
use crate::scene::compose::Composition;
use crate::scene::sequence::SeqState;
use crate::{audio, score};

/// The loaded score (`MARTIN_SCORE` file or built-in), shared for live-audio rendering.
#[derive(Resource, Clone)]
pub(crate) struct ScoreRes(pub std::sync::Arc<score::Score>);

/// Sync gate: while live audio is wanted but the synth render hasn't finished, the show clock
/// (`advance_seq_clock`) holds at 0 — starting the picture without the music would put every
/// `@@` anchor out of sync by the render time. Not inserted when muted/recording (no gate).
#[derive(Resource, Default)]
pub(crate) struct AudioGate {
    pub ready: bool,
}

/// Cinder's synth, rendered on a **background thread** (the render takes seconds; blocking startup
/// stalls the first frame long enough to lose the Vulkan swapchain → crash). `music_director` picks
/// up the WAV bytes when the thread finishes and spawns the player in sync with the show.
#[derive(Resource)]
struct Music {
    // Mutex so the !Sync Receiver can live in a (Send+Sync) Bevy resource.
    rx: std::sync::Mutex<std::sync::mpsc::Receiver<Vec<u8>>>,
    handle: Option<Handle<AudioSource>>,
    entity: Option<Entity>,
    prev_t: f32,
}

/// Startup: kick off the background synth render (unless recording / screenshotting / muted — the
/// recorder muxes the WAV separately). `music_director` picks up the bytes when ready.
fn spawn_synth(mut commands: Commands, score_res: Res<ScoreRes>) {
    let want_audio = std::env::var("MARTIN_RECORD").is_err()
        && std::env::var("MARTIN_SHOT").is_err()
        && std::env::var("MARTIN_MUTE").is_err();
    if !want_audio {
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    if let Ok(path) = std::env::var("MARTIN_MUSIC") {
        // Pre-rendered (bundled) audio — load instantly so it plays in sync, no ~30s synth render.
        match std::fs::read(&path) {
            Ok(bytes) => {
                let _ = tx.send(bytes);
                info!("live audio: playing pre-rendered track ({path})");
            }
            Err(e) => warn!("live audio: MARTIN_MUSIC {path}: {e} — falling back to live render"),
        }
    } else {
        let score = score_res.0.clone();
        std::thread::spawn(move || {
            let _ = tx.send(audio::encode_wav(&audio::synth_track(&score)));
        });
    }
    commands.insert_resource(Music {
        rx: std::sync::Mutex::new(rx),
        handle: None,
        entity: None,
        prev_t: 0.0,
    });
    commands.insert_resource(AudioGate::default());
    info!(
        "live audio: rendering Cinder's synth in the background — the show holds until it's ready \
         (MARTIN_MUTE=1 to skip, MARTIN_MUSIC=<wav> for an instant pre-rendered start)"
    );
}

/// Live playback: turn the background-rendered WAV bytes into an `AudioSource` when ready, spawn it
/// once the sequence is built (so it starts in time with the show), and restart it on a clock reset
/// (Space). Only present when windowed — recording / screenshot / mute don't insert `Music`.
fn music_director(
    mut commands: Commands,
    music: Option<ResMut<Music>>,
    gate: Option<ResMut<AudioGate>>,
    state: Option<Res<SeqState>>,
    comp: Option<Res<Composition>>,
    clock: Res<SeqClock>,
    mut audio_assets: ResMut<Assets<AudioSource>>,
) {
    let Some(mut music) = music else { return };
    // background render finished → make an AudioSource from its WAV bytes (once) + open the gate
    // so the show clock starts: picture and music leave together from t=0.
    if music.handle.is_none() {
        let received = music.rx.lock().unwrap().try_recv().ok();
        if let Some(bytes) = received {
            music.handle = Some(audio_assets.add(AudioSource {
                bytes: bytes.into(),
            }));
            if let Some(mut gate) = gate {
                gate.ready = true;
            }
            info!("live audio: synth ready — show starts");
        }
    }
    // clock jumped backwards (Space restart) → despawn so it respawns from the top, resynced.
    if clock.t + 0.05 < music.prev_t {
        if let Some(e) = music.entity.take() {
            commands.entity(e).despawn();
        }
    }
    music.prev_t = clock.t;
    let built = state.map(|s| s.built).unwrap_or(false) || comp.map(|c| c.built).unwrap_or(false);
    if built && music.entity.is_none() {
        if let Some(h) = music.handle.clone() {
            music.entity = Some(
                commands
                    .spawn((AudioPlayer(h), PlaybackSettings::ONCE))
                    .id(),
            );
        }
    }
}

/// Background synth render + in-sync live playback (needs `ScoreRes` inserted by `main`).
pub(crate) struct MusicPlugin;

impl Plugin for MusicPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_synth)
            .add_systems(Update, music_director);
    }
}
