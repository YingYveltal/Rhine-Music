use crate::library::Track;
use anyhow::Context;
use crossbeam_channel::{Receiver, Sender};
use rodio::{Decoder, OutputStream, Sink, Source};
use serde::Serialize;
use std::{
    fs::File,
    io::BufReader,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use std::sync::atomic::{AtomicU64,Ordering};
pub type Resolver=Arc<dyn Fn(&Track,&AtomicU64,u64)->anyhow::Result<Track>+Send+Sync>;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Playback {
    pub track: Option<Track>,
    pub playing: bool,
    pub elapsed: f64,
    pub error: Option<String>,
    pub transport: String,
    pub bgm_playing: bool,
    pub bgm_error: Option<String>,
    pub applied_command: u64,
    pub worker_error: Option<String>,
    pub current_index: usize,
    pub queue_generation: u64,
}
enum Command {
    Play(Vec<Track>, usize),
    Toggle,
    Stop,
    Seek(f64),
    Unlock,
    Volume(f32, f32, f32, bool, bool),
    External(bool),
    Quit,
}
struct QueuedCommand {
    sequence: u64,
    command: Command,
}
pub struct Audio {
    tx: Sender<QueuedCommand>,
    sequence: Mutex<u64>,
    state: Arc<Mutex<Playback>>,
}
impl Audio {
    pub fn new(assets: PathBuf) -> Self {
        Self::with_resolver(assets,None)
    }
    pub fn with_resolver(assets:PathBuf,resolver:Option<Resolver>)->Self{
        let (tx, rx) = crossbeam_channel::unbounded();
        let state = Arc::new(Mutex::new(Playback::default()));
        let out = state.clone();
        std::thread::spawn(move || worker(rx, out, assets,resolver));
        Self { tx, state, sequence: Mutex::new(0) }
    }
    fn send(&self, command: Command) -> anyhow::Result<u64> {
        // Keep allocation and enqueue under one lock, including concurrent callers.
        let mut sequence = self.sequence.lock().unwrap();
        let next = *sequence + 1;
        self.tx.send(QueuedCommand { sequence: next, command })
            .map_err(|_| anyhow::anyhow!("音频线程不可用"))?;
        *sequence = next;
        Ok(next)
    }
    pub fn play(&self, tracks: Vec<Track>, index: usize) -> anyhow::Result<u64> {
        self.send(Command::Play(tracks, index))
    }
    pub fn toggle(&self) -> anyhow::Result<u64> {
        self.send(Command::Toggle)
    }
    pub fn stop(&self) -> anyhow::Result<u64> {
        self.send(Command::Stop)
    }
    pub fn seek(&self, seconds: f64) -> anyhow::Result<u64> {
        anyhow::ensure!(seconds.is_finite(), "位置必须为有限数字");
        self.send(Command::Seek(seconds.max(0.)))
    }
    pub fn unlock(&self) -> anyhow::Result<u64> {
        self.send(Command::Unlock)
    }
    pub fn settings(&self, volume: f32, bgm: f32, effect: f32, enabled: bool, fade: bool) -> anyhow::Result<u64> {
        self.send(Command::Volume(volume, bgm, effect, enabled, fade))
    }
    pub fn snapshot(&self) -> Playback {
        self.state.lock().unwrap().clone()
    }
    // A source-switch barrier: invalidate preparation, drain the song and pause
    // BGM without changing the user's saved BGM preference.
    pub fn external(&self, active: bool) -> anyhow::Result<u64> { self.send(Command::External(active)) }
}
impl Drop for Audio {
    fn drop(&mut self) {
        let _ = self.send(Command::Quit);
    }
}
fn startup_failed(state: &Mutex<Playback>, error: String) {
    let mut state = state.lock().unwrap();
    state.transport = "error".into();
    state.error = Some(error.clone());
    state.worker_error = Some(error);
}
fn worker(rx: Receiver<QueuedCommand>, state: Arc<Mutex<Playback>>, assets: PathBuf,resolver:Option<Resolver>) {
    let (_stream, handle) = match OutputStream::try_default() {
        Ok(v) => v,
        Err(e) => {
            startup_failed(&state, format!("无法打开音频设备：{e}"));
            return;
        }
    };
    let sink = match Sink::try_new(&handle) {
        Ok(s) => s,
        Err(e) => {
            startup_failed(&state, e.to_string());
            return;
        }
    };
    let bgm = Sink::try_new(&handle).ok();
    if let Some(bgm) = &bgm {
        match File::open(assets.join("audio/atmosphere.ogg"))
            .map_err(|e| e.to_string())
            .and_then(|file| Decoder::new(BufReader::new(file)).map_err(|e| e.to_string()))
        {
            Ok(source) => {
                bgm.append(source.repeat_infinite());
                bgm.set_volume(0.0);
            }
            Err(e) => state.lock().unwrap().bgm_error = Some(e),
        }
    }
    run_transport(rx, state, sink, bgm, resolver, || Sink::try_new(&handle));
}

// The device setup stays above; the same transport loop can use idle sinks in
// tests without opening an output device or duplicating its transitions.
fn run_transport(
    rx: Receiver<QueuedCommand>,
    state: Arc<Mutex<Playback>>,
    mut sink: Sink,
    bgm: Option<Sink>,
    resolver: Option<Resolver>,
    mut new_sink: impl FnMut() -> Result<Sink, rodio::PlayError>,
) {
    let mut volume = 0.72;
    let mut bgm_volume = 0.18;
    let mut bgm_enabled = true;
    let mut fade = true;
    // Retain the resolved/decoded metadata for the source currently in the sink.
    let mut loaded_track: Option<Track> = None;
    let mut queue = Vec::<Track>::new();
    let mut index = 0;
    let mut pending = None;
    let epoch=Arc::new(AtomicU64::new(0));
    let mut loading:Option<Receiver<anyhow::Result<Track>>>=None;
    let mut phase = 0_u8;
    let mut phase_at = Instant::now();
    let mut gain = 1.0_f32;
    let mut fade_from = 1.0_f32;
    let mut bgm_gain = 0.0_f32;
    let mut stopped = true;
    let mut unlocked = false;
    let mut bgm_transition = Instant::now();
    let mut bgm_from = 0.;
    let mut bgm_target = 0.;
    let mut external = false;
    let mut drain_receipt = None;
    state.lock().unwrap().transport = "idle".into();
    loop {
        let mut applied = None;
        if let Ok(queued) = rx.recv_timeout(Duration::from_millis(10)) {
            applied = Some(queued.sequence);
            match queued.command {
                Command::Quit => break,
                Command::Play(tracks, i) => {
                    if i < tracks.len() {
                        epoch.fetch_add(1,Ordering::SeqCst);loading=None;
                        unlocked = true;
                        queue = tracks;
                        index = i;
                        state.lock().unwrap().queue_generation += 1;
                        let same = loaded_track.as_ref().is_some_and(|track| track.id == queue[i].id);
                        if same && !sink.empty() {
                            sink.play();
                            gain = 1.;
                            phase = 0;
                            pending = None;
                            let mut s = state.lock().unwrap();
                            s.track = loaded_track.clone();
                            s.transport = "playing".into();
                            s.playing = true;
                            s.elapsed = sink.get_pos().as_secs_f64();
                            s.error = None;
                        } else {
                            pending = Some(queue[i].clone());
                            fade_from = gain;
                            phase = if fade && !sink.empty() && !sink.is_paused() {
                                1
                            } else {
                                2
                            };
                            phase_at = Instant::now();
                            {
                                let mut s = state.lock().unwrap();
                                s.transport = "loading".into();
                                s.track = Some(queue[i].clone());
                                s.error = None;
                                s.elapsed = 0.;
                            }
                        }
                        stopped = false;
                    }
                }
                Command::Toggle => {
                    epoch.fetch_add(1,Ordering::SeqCst);loading=None;
                    if pending.is_some() {
                        let selected = state.lock().unwrap().track.as_ref().map(|t| t.id.clone());
                        if loaded_track.as_ref().map(|track| &track.id) != selected.as_ref() {
                            sink.stop();
                            loaded_track = None;
                        }
                    }
                    pending = None;
                    phase = 0;
                    gain = 1.0;
                    // stop() drains on the output callback. A discarded source
                    // must not be resumed while those final samples are pending.
                    if loaded_track.is_some() && !sink.empty() {
                        if sink.is_paused() {
                            sink.play();
                            state.lock().unwrap().transport = "playing".into();
                        } else {
                            sink.pause();
                            state.lock().unwrap().transport = "paused".into();
                        }
                    } else {
                        state.lock().unwrap().transport = "paused".into();
                    }
                }
                Command::Stop => {
                    epoch.fetch_add(1,Ordering::SeqCst);loading=None;
                    sink.stop();
                    loaded_track = None;
                    pending = None;
                    queue.clear();
                    phase = 0;
                    stopped = true;
                    let mut s = state.lock().unwrap();
                    s.playing = false;
                    s.elapsed = 0.0;
                    s.error = None;
                    s.transport = "idle".into();
                }
                Command::Seek(seconds) => {
                    if let Err(e) = sink.try_seek(Duration::from_secs_f64(seconds)) {
                        state.lock().unwrap().error = Some(e.to_string());
                    }
                }
                Command::Unlock => unlocked = true,
                Command::Volume(v, b, _e, on, f) => {
                    volume = v;
                    bgm_volume = b;
                    bgm_enabled = on;
                    fade = f;
                }
                Command::External(active) => {
                    external = active;
                    if active {
                        epoch.fetch_add(1, Ordering::SeqCst); loading = None;
                        sink.set_volume(0.); sink.play(); sink.stop();
                        loaded_track = None; pending = None; queue.clear(); phase = 0; stopped = true;
                        bgm_gain = 0.; bgm_from = 0.; bgm_target = 0.;
                        if let Some(bgm) = &bgm { bgm.set_volume(0.); bgm.pause(); }
                        drain_receipt = applied.take();
                        let mut s = state.lock().unwrap();
                        s.playing = false; s.elapsed = 0.; s.transport = "idle".into(); s.error = None;
                    } else if let Some(bgm) = &bgm { bgm.play(); }
                }
            }
        }
        if phase == 1 {
            let t = (phase_at.elapsed().as_secs_f32() / 0.45).min(1.0);
            gain = fade_from * (1.0 - t);
            if t >= 1.0 {
                phase = 2;
            }
        }
        if phase == 2 && bgm_gain < 0.001 {
            if pending.as_ref().is_some_and(|t:&Track|t.id.starts_with("qq-")) {
                let track=pending.take().unwrap();
                let (sender,receiver)=crossbeam_channel::bounded(1);loading=Some(receiver);
                let resolver=resolver.clone();let token=epoch.clone();let expected=token.load(Ordering::SeqCst);
                {let mut s=state.lock().unwrap();s.track=Some(track.clone());s.transport="loading".into();s.elapsed=0.;s.playing=false;}
                sink.stop();loaded_track=None;
                std::thread::spawn(move||{let result=resolver.context("QQ 音乐播放通道尚未连接").and_then(|r|r(&track,&token,expected));let _=sender.send(result);});
                phase=4;
            }
        }
        if phase==4 {
            if let Some(result)=loading.as_ref().and_then(|r|r.try_recv().ok()){
                loading=None;
                match result {Ok(track)=>{pending=Some(track);phase=5;},Err(e)=>{phase=0;queue.clear();let mut s=state.lock().unwrap();s.transport="error".into();s.error=Some(e.to_string());s.playing=false;}}
            }
        }
        if (phase == 2 || phase==5) && bgm_gain < 0.001 {
            if let Some(mut track) = pending.take() {
                let load = (|| -> anyhow::Result<_> {
                    if !track.browser_playable {
                        anyhow::bail!("{} 暂不支持播放", track.format)
                    }
                    let source:Box<dyn Source<Item=i16>+Send>=if track.id.starts_with("qq-"){Box::new(crate::qq_audio::QqAudio::open(&track.path)?)}else{Box::new(Decoder::new(BufReader::new(File::open(&track.path)?))?)};
                    if let Some(duration)=source.total_duration(){if track.duration<=0.{track.duration=duration.as_secs_f64()}}
                    let new = new_sink()?;
                    new.set_volume(0.0);
                    new.append(source);
                    Ok(new)
                })();
                match load {
                    Ok(next) => {
                        sink.stop();
                        sink = next;
                        loaded_track = Some(track.clone());
                        let mut s = state.lock().unwrap();
                        s.track = Some(track);
                        s.error = None;
                        s.playing = true;
                        s.transport = "playing".into();
                        phase = if fade { 3 } else { 0 };
                        gain = if fade { 0.0 } else { 1.0 };
                        phase_at = Instant::now();
                    }
                    Err(e) => {
                        sink.stop();
                        loaded_track = None;
                        phase = 0;
                        let mut s = state.lock().unwrap();
                        s.error = Some(format!("无法播放 {}：{e}", track.title));
                        s.transport = "error".into();
                        stopped = false;
                        queue.clear();
                    }
                }
            } else {
                phase = 0;
            }
        }
        if phase == 3 {
            gain = (phase_at.elapsed().as_secs_f32() / 0.45).min(1.0);
            if gain >= 1.0 {
                phase = 0;
            }
        }
        sink.set_volume(volume * gain);
        if phase == 0 && !stopped && sink.empty() && !queue.is_empty() && state.lock().unwrap().transport=="playing" {
            if index + 1 < queue.len() {
                index += 1;
                pending = Some(queue[index].clone());
                phase = 2;
            } else {
                stopped = true;
                queue.clear();
                state.lock().unwrap().transport = "idle".into();
            }
        }
        let target = if !external && stopped && bgm_enabled && unlocked {
            1.0
        } else {
            0.0
        };
        if target != bgm_target {
            bgm_target = target;
            bgm_from = bgm_gain;
            bgm_transition = Instant::now();
        }
        let duration = if target > bgm_from { 0.65 } else { 0.22 };
        bgm_gain = bgm_from
            + (target - bgm_from) * (bgm_transition.elapsed().as_secs_f32() / duration).min(1.);
        if let Some(bgm) = &bgm {
            bgm.set_volume(bgm_volume * bgm_gain);
        }
        let mut s = state.lock().unwrap();
        s.playing = s.transport == "playing" && !sink.is_paused() && !sink.empty();
        s.elapsed = if stopped || s.transport == "loading" {
            0.
        } else {
            sink.get_pos().as_secs_f64()
        };
        s.bgm_playing = bgm_gain > 0.001 && bgm.is_some();
        s.current_index = index;
        if let Some(waiting) = drain_receipt.as_mut() {
            if let Some(newer) = applied.take() { *waiting = (*waiting).max(newer); }
        }
        if drain_receipt.is_some() && sink.empty() && bgm.as_ref().is_none_or(|b| b.is_paused()) {
            applied = drain_receipt.take();
        }
        // Publish the acknowledgment only with this iteration's final snapshot.
        if let Some(sequence) = applied { s.applied_command = sequence; }
    }
    epoch.fetch_add(1,Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decodes_bundled_audio_without_speakers() {
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../frontend/public");
        for name in ["atmosphere.ogg", "pulse.ogg", "observatory-preview.mp3"] {
            let mut d = Decoder::new(BufReader::new(
                File::open(assets.join("audio").join(name)).unwrap(),
            ))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(d.next().is_some());
            assert!(d.sample_rate() > 0);
        }
    }
    #[test]
    #[ignore = "requires a real macOS audio device; always muted"]
    fn native_transport_pause_seek_queue_and_stop() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wav");
        let mut wav = hound::WavWriter::create(
            &path,
            hound::WavSpec {
                channels: 1,
                sample_rate: 8000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for _ in 0..80000 {
            wav.write_sample(0_i16).unwrap();
        }
        wav.finalize().unwrap();
        let track = Track {
            id: "test".into(),
            title: "Test".into(),
            path,
            duration: 10.,
            browser_playable: true,
            ..Default::default()
        };
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../frontend/public");
        let player = Audio::new(assets);
        player.settings(0., 0., 0., false, false).unwrap();
        let wait = |predicate: &dyn Fn(&Playback) -> bool| {
            let start = Instant::now();
            loop {
                let s = player.snapshot();
                assert!(s.error.is_none(), "{:?}", s.error);
                if predicate(&s) {
                    break;
                }
                assert!(start.elapsed() < Duration::from_secs(4), "state: {s:?}");
                std::thread::sleep(Duration::from_millis(20));
            }
        };
        player.play(vec![track.clone()], 0).unwrap();
        wait(&|s| s.playing && s.elapsed > 0.1);
        player.toggle().unwrap();
        wait(&|s| s.transport == "paused" && !s.playing);
        player.seek(3.).unwrap();
        wait(&|s| (s.elapsed - 3.).abs() < 0.1);
        player.play(vec![track.clone()], 0).unwrap();
        wait(&|s| s.playing && s.elapsed > 3.1);
        let mut next = track.clone();
        next.id = "next".into();
        player.play(vec![track, next], 0).unwrap();
        player.seek(9.9).unwrap();
        wait(&|s| s.track.as_ref().is_some_and(|t| t.id == "next") && s.playing);
        player.stop().unwrap();
        wait(&|s| s.transport == "idle" && !s.playing && s.elapsed == 0.);
    }
}

#[cfg(test)]
#[path = "audio/transport_tests.rs"]
mod transport_tests;
