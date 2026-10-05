use super::*;
use std::thread::JoinHandle;

type Output = rodio::queue::SourcesQueueOutput<f32>;
struct Transport {
    tx: Sender<Command>,
    state: Arc<Mutex<Playback>>,
    outputs: Receiver<Output>,
    thread: Option<JoinHandle<()>>,
}
impl Transport {
    fn new(resolver: Option<Resolver>) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        let (output_tx, outputs) = crossbeam_channel::unbounded();
        let state = Arc::new(Mutex::new(Playback::default()));
        let worker_state = state.clone();
        let thread = std::thread::spawn(move || {
            let (sink, _initial_output) = Sink::new_idle();
            run_transport(rx, worker_state, sink, None, resolver, || {
                let (sink, output) = Sink::new_idle();
                output_tx.send(output).unwrap();
                Ok(sink)
            });
        });
        Self { tx, state, outputs, thread: Some(thread) }
    }
    fn send(&self, command: Command) { self.tx.send(command).unwrap(); }
    fn play(&self, queue: &[Track], index: usize) { self.send(Command::Play(queue.to_vec(), index)); }
    fn wait(&self, predicate: impl Fn(&Playback) -> bool) -> Playback {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let state = self.state.lock().unwrap().clone();
            if predicate(&state) { return state; }
            assert!(Instant::now() < deadline, "transport did not settle: {state:?}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn playing(&self, id: &str) -> Playback {
        self.wait(|s| s.playing && s.track.as_ref().is_some_and(|t| t.id == id))
    }
    fn output(&self) -> Output { self.outputs.recv_timeout(Duration::from_secs(3)).unwrap() }
}
impl Drop for Transport {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Quit);
        self.thread.take().unwrap().join().unwrap();
    }
}
fn wav(dir: &std::path::Path, name: &str, frames: usize) -> Track {
    let path = dir.join(format!("{name}.wav"));
    let mut file = hound::WavWriter::create(&path, hound::WavSpec {
        channels: 1, sample_rate: 8000, bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    }).unwrap();
    for _ in 0..frames { file.write_sample(0_i16).unwrap(); }
    file.finalize().unwrap();
    Track { id: name.into(), title: name.into(), path, browser_playable: true, ..Default::default() }
}
fn consume(output: &mut Output, samples: usize) {
    // Consume decoded samples in memory. No OutputStream, speakers or wall-clock
    // playback is involved; the product Sink updates its actual position.
    for _ in 0..samples { assert!(output.next().is_some()); }
}

#[test]
fn returning_during_fade_restores_the_loaded_qq_metadata_and_position() {
    let dir = tempfile::tempdir().unwrap();
    let mut resolved = wav(dir.path(), "qq-a", 80000);
    resolved.format = "AAC".into();
    resolved.codec = Some("AAC".into());
    let expected_path = resolved.path.clone();
    let calls = Arc::new(AtomicU64::new(0));
    let count = calls.clone();
    let resolver: Resolver = Arc::new(move |_, _, _| {
        count.fetch_add(1, Ordering::SeqCst);
        Ok(resolved.clone())
    });
    let a = Track { id: "qq-a".into(), title: "A".into(), browser_playable: true,
        format: "QQ 音乐".into(), ..Default::default() };
    let b = wav(dir.path(), "b", 80000);
    let player = Transport::new(Some(resolver));
    player.play(&[a.clone(), b.clone()], 0);
    player.playing("qq-a");
    let mut output = player.output();
    consume(&mut output, 2000);
    let before = player.wait(|s| s.elapsed > 0.1).elapsed;
    player.play(&[a.clone(), b.clone()], 1);
    player.wait(|s| s.transport == "loading" && s.track.as_ref().unwrap().id == "b");
    player.play(&[b, a], 1); // The resumed item is now at a different queue index.
    let resumed = player.playing("qq-a");
    let track = resumed.track.unwrap();
    assert_eq!(track.path, expected_path);
    assert_eq!(track.duration, 10.0); // Filled by the actual decoder, absent from the queue item.
    assert_eq!(track.codec.as_deref(), Some("AAC"));
    assert_eq!(track.format, "AAC");
    assert!(resumed.elapsed >= before);
    assert!(resumed.error.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 1, "resume must not resolve A again");
    assert!(player.outputs.is_empty(), "resume must reuse the loaded source");
    consume(&mut output, 82000);
    player.wait(|s| s.transport == "idle"); // Resumed A was last, so B must not run later.
    assert!(player.outputs.is_empty());
}

#[test]
fn paused_source_resumes_and_clears_a_previous_error() {
    let dir = tempfile::tempdir().unwrap();
    let a = wav(dir.path(), "a", 80000);
    let player = Transport::new(None);
    player.play(&[a.clone()], 0);
    player.playing("a");
    let mut output = player.output();
    consume(&mut output, 2000);
    let position = player.wait(|s| s.elapsed > 0.1).elapsed;
    player.send(Command::Toggle);
    player.wait(|s| s.transport == "paused" && !s.playing);
    // A previous seek error can coexist with a paused, usable source.
    player.state.lock().unwrap().error = Some("synthetic previous seek failure".into());
    player.play(&[a], 0);
    let state = player.playing("a");
    assert!(state.error.is_none());
    assert!(state.elapsed >= position);
    assert_eq!(state.track.unwrap().duration, 10.0);
    assert!(player.outputs.is_empty());
}

#[test]
fn ordinary_switch_keeps_fade_and_automatic_queue_order() {
    let dir = tempfile::tempdir().unwrap();
    let queue = [wav(dir.path(), "a", 80000), wav(dir.path(), "b", 1600), wav(dir.path(), "c", 80000)];
    let player = Transport::new(None);
    player.play(&queue, 0);
    player.playing("a");
    let _a_output = player.output();
    player.play(&queue, 1);
    player.wait(|s| s.transport == "loading" && s.track.as_ref().unwrap().id == "b");
    assert!(player.outputs.is_empty(), "B must not load before A fades out");
    player.playing("b");
    let mut b_output = player.output();
    consume(&mut b_output, 3000);
    player.playing("c");
    let _c_output = player.output();
    player.send(Command::Stop);
    let stopped = player.wait(|s| s.transport == "idle");
    assert!(!stopped.playing);
    assert_eq!(stopped.elapsed, 0.0);
}

#[test]
fn cancelling_a_pending_switch_from_paused_a_can_load_a_again() {
    let dir = tempfile::tempdir().unwrap();
    let a = wav(dir.path(), "a", 80000);
    let b = Track { id: "qq-b".into(), browser_playable: true, ..Default::default() };
    let (started_tx, started) = crossbeam_channel::bounded(1);
    let (release, gate) = crossbeam_channel::bounded(1);
    let (finished_tx, finished) = crossbeam_channel::bounded(1);
    let resolver: Resolver = Arc::new(move |_, token, version| {
        started_tx.send(()).unwrap();
        gate.recv_timeout(Duration::from_secs(3)).unwrap();
        finished_tx.send(token.load(Ordering::SeqCst) != version).unwrap();
        anyhow::bail!("obsolete B failure")
    });
    let player = Transport::new(Some(resolver));
    player.play(&[a.clone(), b.clone()], 0);
    player.playing("a");
    let _a_output = player.output();
    player.send(Command::Toggle);
    player.wait(|s| s.transport == "paused");
    player.play(&[a.clone(), b], 1);
    started.recv_timeout(Duration::from_secs(3)).unwrap();
    player.send(Command::Toggle); // Pause while B's resolver is pending.
    player.wait(|s| s.transport == "paused");
    player.play(&[a], 0);
    player.playing("a");
    let _reloaded_a_output = player.output();
    release.send(()).unwrap();
    assert!(finished.recv_timeout(Duration::from_secs(3)).unwrap());
    player.send(Command::Toggle);
    let state = player.wait(|s| s.transport == "paused");
    assert_eq!(state.track.unwrap().id, "a");
    assert!(state.error.is_none());
    assert!(player.outputs.is_empty());
}

#[test]
fn superseding_or_stopping_a_resolver_discards_its_late_result() {
    for stop in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let resolved_a = wav(dir.path(), "qq-a", 80000);
        let resolved_b = wav(dir.path(), "qq-b", 80000);
        let a = Track { id: "qq-a".into(), browser_playable: true, ..Default::default() };
        let b = Track { id: "qq-b".into(), browser_playable: true, ..Default::default() };
        let (started_tx, started) = crossbeam_channel::bounded(1);
        let (release, gate) = crossbeam_channel::bounded(1);
        let (finished_tx, finished) = crossbeam_channel::bounded(1);
        let resolver: Resolver = Arc::new(move |track, token, version| {
            if track.id == "qq-a" { return Ok(resolved_a.clone()); }
            started_tx.send(()).unwrap();
            gate.recv_timeout(Duration::from_secs(3)).unwrap();
            finished_tx.send(token.load(Ordering::SeqCst) != version).unwrap();
            if stop { anyhow::bail!("obsolete B error") }
            Ok(resolved_b.clone())
        });
        let player = Transport::new(Some(resolver));
        player.send(Command::Volume(0.7, 0.0, 0.0, false, false));
        player.play(&[a.clone(), b.clone()], 0);
        player.playing("qq-a");
        let _a_output = player.output();
        player.play(&[a.clone(), b], 1);
        started.recv_timeout(Duration::from_secs(3)).unwrap();
        if stop {
            player.send(Command::Stop);
            player.wait(|s| s.transport == "idle");
        } else {
            player.play(&[a], 0);
            player.playing("qq-a");
            let _reloaded_a_output = player.output();
        }
        release.send(()).unwrap();
        assert!(finished.recv_timeout(Duration::from_secs(3)).unwrap(), "old epoch must be invalid");
        // Exercise a subsequent worker command after the obsolete resolver returns.
        if !stop { player.send(Command::Toggle); }
        let state = player.wait(|s| s.transport == if stop { "idle" } else { "paused" });
        assert!(!state.playing);
        assert!(state.error.is_none());
        if stop { assert_eq!(state.elapsed, 0.0); }
        else { assert_eq!(state.track.unwrap().id, "qq-a"); }
        assert!(player.outputs.is_empty(), "obsolete B must never acquire a sink");
    }
}
