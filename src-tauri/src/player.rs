//! One receipt stream for the Rust and MusicKit transports. A switch is a
//! completion barrier, never a fixed delay or a UI-only source flag.
use crate::{apple::{Native, Response}, audio::{Audio, Playback}, library::Track};
use anyhow::{bail, Result};
use crossbeam_channel::{Receiver, Sender};
use serde_json::{json, Value};
use std::{collections::BTreeSet, sync::{Arc, Mutex}, time::{Duration, Instant}};

#[derive(Clone)]
pub enum Command {
    Play(Vec<Track>, usize), ApplePlay { id: String, ids: Vec<String> },
    Toggle, Stop, DisableApple, Seek(f64), Next, Previous, Unlock,
    Settings(f32, f32, f32, bool, bool),
}
pub trait Local: Send + Sync {
    fn send(&self, command: &Command) -> Result<u64>;
    fn external(&self, active: bool) -> Result<u64>;
    fn snapshot(&self) -> Playback;
}
impl Local for Audio {
    fn send(&self, command: &Command) -> Result<u64> {
        match command {
            Command::Play(t,i) => self.play(t.clone(),*i), Command::Toggle => self.toggle(),
            Command::Stop => self.stop(), Command::Seek(v) => self.seek(*v), Command::Unlock => self.unlock(),
            Command::Settings(v,b,e,on,f) => self.settings(*v,*b,*e,*on,*f),
            _ => bail!("该来源不支持原生切歌"),
        }
    }
    fn external(&self, active: bool) -> Result<u64> { self.external(active) }
    fn snapshot(&self) -> Playback { self.snapshot() }
}
struct Envelope { receipt: u64, command: Command }
pub struct Player { tx: Sender<Envelope>, sequence: Mutex<u64>, state: Arc<Mutex<Value>> }
impl Player {
    pub fn new(local: Arc<dyn Local>, native: Arc<dyn Native>) -> Self {
        let (tx,rx)=crossbeam_channel::unbounded();
        let state=Arc::new(Mutex::new(json!({"track":null,"playing":false,"elapsed":0,"transport":"idle",
            "error":null,"workerError":null,"bgmPlaying":false,"bgmError":null,"appliedCommand":0,
            "source":null,"currentIndex":0,"queueGeneration":0,"capabilities":{"seek":true,"volume":true,"fade":true}})));
        let output=state.clone(); std::thread::spawn(move||Driver::new(local,native).run(rx,output));
        Self {tx,sequence:Mutex::new(0),state}
    }
    pub fn send(&self, command: Command) -> Result<u64> {
        let mut sequence=self.sequence.lock().unwrap(); let next=*sequence+1;
        self.tx.send(Envelope {receipt:next,command}).map_err(|_|anyhow::anyhow!("播放器线程不可用"))?;
        *sequence=next; Ok(next)
    }
    pub fn snapshot(&self) -> Value { self.state.lock().unwrap().clone() }
}
enum Waiting {
    Barrier { native: Option<Response>, native_done: bool, local: u64, then: Option<Command> },
    Native(Response), Local { receipt: u64, loading: bool },
}
struct Pending { receipt: u64, command: Command, wait: Waiting }
struct Driver {
    local: Arc<dyn Local>, native: Arc<dyn Native>, source: Option<String>, apple_owned: bool,
    pending: Option<Pending>, superseded: Vec<u64>, auxiliary: Vec<(u64,u64)>,
    done: BTreeSet<u64>, applied: u64, error: Option<String>,
    apple_state: Value, state_request: Option<Response>, last_poll: Instant,
}
impl Driver {
    fn new(local:Arc<dyn Local>,native:Arc<dyn Native>)->Self {
        Self {local,native,source:None,apple_owned:false,pending:None,superseded:vec![],auxiliary:vec![],done:BTreeSet::new(),
            applied:0,error:None,apple_state:json!({"track":null,"playing":false,"transport":"idle","elapsed":0,"currentIndex":0,"queueGeneration":0}),
            state_request:None,last_poll:Instant::now()}
    }
    fn complete(&mut self, receipt:u64) {
        self.done.insert(receipt);
        for old in self.superseded.drain(..) { self.done.insert(old); }
        while self.done.remove(&(self.applied+1)) { self.applied+=1; }
    }
    fn native_command(&self,command:&Command)->Response {
        match command {
            Command::ApplePlay{id,ids}=>self.native.call("play",json!({"id":id,"ids":ids})),
            Command::Toggle=>self.native.call("toggle",json!({})), Command::Next=>self.native.call("next",json!({})),
            Command::Previous=>self.native.call("previous",json!({})), Command::Seek(value)=>self.native.call("seek",json!({"value":value})),
            _=>self.native.call("stop",json!({})),
        }
    }
    fn accept(&mut self, mut e:Envelope) {
        if matches!(e.command,Command::DisableApple) {
            if self.apple_owned { e.command=Command::Stop; }
            else {self.done.insert(e.receipt);return;}
        }
        if matches!(e.command,Command::Settings(..)|Command::Unlock) {
            match self.local.send(&e.command) {
                Ok(receipt)=>self.auxiliary.push((e.receipt,receipt)),
                Err(error)=>{self.error=Some(error.to_string());self.done.insert(e.receipt);}
            }
            return;
        }
        // A newer transport command explicitly supersedes the older command.
        // Its receipt is acknowledged only when the new operation has settled.
        let was_switching=self.pending.as_ref().is_some_and(|p|matches!(p.wait,Waiting::Barrier{..}));
        // Seek/skip has no meaningful target before the new queue exists. Leave
        // the pending switch intact; the rejected command still gets a receipt.
        if was_switching && matches!(e.command,Command::Seek(..)|Command::Next|Command::Previous) {
            self.error=Some("正在切换音乐来源，请稍后重试".into());self.done.insert(e.receipt);return;
        }
        if self.pending.is_none() && self.source.as_deref()==Some("apple") && !self.apple_owned && matches!(e.command,Command::Toggle|Command::Next|Command::Previous|Command::Seek(..)) {
            self.complete(e.receipt);return;
        }
        self.state_request=None;
        if let Some(old)=self.pending.take() { self.superseded.push(old.receipt); }
        self.error=None;
        let new_apple=matches!(e.command,Command::ApplePlay{..});
        let new_local=matches!(e.command,Command::Play(..));
        let stop=matches!(e.command,Command::Stop);
        let switch=new_apple || ((new_local||stop) && self.apple_owned) || was_switching;
        let wait=if switch {
            match self.local.external(true) {
                Ok(local)=>Waiting::Barrier {native:if self.apple_owned||new_apple {Some(self.native.call("stop",json!({})))} else {None},
                    native_done:!self.apple_owned&&!new_apple,local,then:if stop || (was_switching && matches!(e.command,Command::Toggle)) {None} else {Some(e.command.clone())}},
                Err(error)=>{self.error=Some(error.to_string());self.complete(e.receipt);return;}
            }
        } else if self.apple_owned && self.source.as_deref()==Some("apple") && !new_local {
            Waiting::Native(self.native_command(&e.command))
        } else {
            match self.local.send(&e.command) {
                Ok(receipt)=>Waiting::Local{receipt,loading:matches!(e.command,Command::Play(..))},
                Err(error)=>{self.error=Some(error.to_string());self.complete(e.receipt);return;}
            }
        };
        if new_apple { self.apple_owned=true; }
        self.pending=Some(Pending{receipt:e.receipt,command:e.command,wait});
    }
    fn advance(&mut self) {
        let snapshot=self.local.snapshot();
        self.auxiliary.retain(|(receipt,local)| {
            if snapshot.applied_command>=*local { self.done.insert(*receipt);false } else {true}
        });
        while self.done.remove(&(self.applied+1)){self.applied+=1;}
        let Some(mut pending)=self.pending.take() else {return};
        let mut result=None;
        match &mut pending.wait {
            Waiting::Barrier{native,native_done,local,then}=>{
                if !*native_done {
                    if let Some(response)=native.as_ref().and_then(|r|r.try_recv().ok()) {
                        match response { Ok(value)=>{self.apple_state=value;*native_done=true},Err(e)=>result=Some(Err(e)) }
                    }
                }
                if snapshot.worker_error.is_some() { result=Some(Err(snapshot.worker_error.clone().unwrap())); }
                if result.is_none() && *native_done && snapshot.applied_command>=*local && !snapshot.playing && !snapshot.bgm_playing {
                    match then.take() {
                        Some(command @ Command::ApplePlay{..})=>{
                            self.source=Some("apple".into());self.apple_owned=true;
                            pending.wait=Waiting::Native(self.native_command(&command));
                        }
                        Some(command)=>{
                            self.apple_owned=false;
                            let sent=self.local.external(false).and_then(|_|self.local.send(&command));
                            match sent {Ok(receipt)=>pending.wait=Waiting::Local{receipt,loading:matches!(command,Command::Play(..))},Err(e)=>result=Some(Err(e.to_string()))}
                        }
                        None=>{
                            self.apple_owned=false;
                            match self.local.external(false).and_then(|_|self.local.send(&Command::Stop)) {
                                Ok(receipt)=>pending.wait=Waiting::Local{receipt,loading:false},Err(e)=>result=Some(Err(e.to_string()))
                            }
                            self.apple_state["transport"]=json!("idle");self.apple_state["playing"]=json!(false);self.apple_state["elapsed"]=json!(0);
                        }
                    }
                }
            }
            Waiting::Native(rx)=>{
                if let Ok(response)=rx.try_recv(){result=Some(response.map(|v|{self.apple_state=v;}));}
            }
            Waiting::Local{receipt,loading}=>{
                if snapshot.worker_error.is_some(){result=Some(Err(snapshot.worker_error.clone().unwrap()));}
                else if snapshot.applied_command>=*receipt && (!*loading||snapshot.transport!="loading") {
                    if matches!(pending.command,Command::Play(..)) {
                        self.source=snapshot.track.as_ref().map(|t|if t.id.starts_with("qq-"){"qq"}else{"local"}.into());
                    }
                    result=Some(snapshot.error.clone().map_or(Ok(()),Err));
                }
            }
        }
        if let Some(result)=result {self.state_request=None;if let Err(e)=result {self.error=Some(e);}self.complete(pending.receipt);}
        else {self.pending=Some(pending);}
    }
    fn snapshot(&mut self)->Value {
        let local=self.local.snapshot();
        if self.source.as_deref()!=Some("apple") {
            if let Some(track)=&local.track { self.source=Some(if track.id.starts_with("qq-"){"qq"}else{"local"}.into()); }
        }
        // At most one read in flight; reads never call pause or synthesize EOF.
        if self.apple_owned && self.pending.is_none() && self.state_request.is_none() && self.last_poll.elapsed()>=Duration::from_millis(100) {
            self.state_request=Some(self.native.call("state",json!({})));self.last_poll=Instant::now();
        }
        if let Some(response)=self.state_request.as_ref().and_then(|r|r.try_recv().ok()) {
            self.state_request=None;
            if let Ok(value)=response {self.apple_state=value;}
        }
        let apple=self.source.as_deref()==Some("apple");
        let mut state=if apple {self.apple_state.clone()} else {serde_json::to_value(&local).unwrap()};
        state["source"]=json!(self.source);state["appliedCommand"]=json!(self.applied);
        state["bgmPlaying"]=json!(local.bgm_playing);state["bgmError"]=json!(local.bgm_error);
        state["workerError"]=if apple {Value::Null}else{json!(local.worker_error)};
        state["capabilities"]=json!({"seek":true,"volume":!apple,"fade":!apple});
        if !state["track"].is_null(){state["track"]["source"]=json!(self.source);}
        if let Some(p)=&self.pending {
            if matches!(p.command,Command::Play(..)|Command::ApplePlay{..}) {
                state["transport"]=json!("loading");state["playing"]=json!(false);
            }
        }
        state["error"]=json!(self.error.clone().or(local.error.filter(|_|!apple)));
        if self.error.is_some(){state["transport"]=json!("error");}
        state
    }
    fn run(mut self, rx:Receiver<Envelope>,output:Arc<Mutex<Value>>) {
        loop {
            match rx.recv_timeout(Duration::from_millis(10)) {
                Ok(e)=>self.accept(e),Err(crossbeam_channel::RecvTimeoutError::Disconnected)=>{let _=self.local.external(true);let _=self.native.call("stop",json!({}));break;},Err(_)=>{}
            }
            self.advance();*output.lock().unwrap()=self.snapshot();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    #[derive(Default)]
    struct FakeLocal {state:Mutex<Playback>,calls:Mutex<Vec<String>>,sequence:Mutex<u64>}
    impl FakeLocal {
        fn issue(&self,name:&str)->u64 { self.calls.lock().unwrap().push(name.into());let mut s=self.sequence.lock().unwrap();*s+=1;*s }
        fn finish(&self,sequence:u64,transport:&str) {let mut s=self.state.lock().unwrap();s.applied_command=sequence;s.transport=transport.into();s.playing=transport=="playing";}
    }
    impl Local for FakeLocal {
        fn send(&self,c:&Command)->Result<u64> {
            if let Command::Play(t,i)=c {self.state.lock().unwrap().track=Some(t[*i].clone());}
            Ok(self.issue(match c {Command::Play(..)=>"play",Command::Stop=>"stop",Command::Settings(..)=>"settings",_=>"control"}))
        }
        fn external(&self,on:bool)->Result<u64>{Ok(self.issue(if on{"suspend"}else{"release"}))}
        fn snapshot(&self)->Playback{self.state.lock().unwrap().clone()}
    }
    #[derive(Default)]
    struct FakeNative { requests:Mutex<VecDeque<(String,Sender<Result<Value,String>>)>> }
    impl Native for FakeNative {
        fn call(&self,op:&str,_:Value)->Response {let(tx,rx)=crossbeam_channel::bounded(1);self.requests.lock().unwrap().push_back((op.into(),tx));rx}
    }
    impl FakeNative {
        fn take(&self,op:&str)->Sender<Result<Value,String>> {let(name,tx)=self.requests.lock().unwrap().pop_front().unwrap();assert_eq!(name,op);tx}
    }
    fn apple_play()->Command {Command::ApplePlay{id:"apple-track-occurrence".into(),ids:vec!["apple-track-occurrence".into()]}}
    fn native_state(transport:&str)->Value {json!({"transport":transport,"playing":transport=="playing","elapsed":0,"currentIndex":0,"queueGeneration":1,"track":{"id":"apple-track-occurrence"}})}
    #[test]
    fn source_switch_waits_for_both_real_completion_barriers() {
        let local=Arc::new(FakeLocal::default());let native=Arc::new(FakeNative::default());
        let mut d=Driver::new(local.clone(),native.clone());
        d.accept(Envelope{receipt:1,command:apple_play()});
        native.take("stop").send(Ok(native_state("idle"))).unwrap();d.advance();
        assert!(native.requests.lock().unwrap().is_empty());assert_eq!(d.applied,0);
        local.finish(1,"idle");d.advance();
        native.take("play").send(Ok(native_state("playing"))).unwrap();d.advance();
        assert_eq!(d.applied,1);assert_eq!(d.source.as_deref(),Some("apple"));
        let track=Track{id:"local-a".into(),..Default::default()};
        d.accept(Envelope{receipt:2,command:Command::Play(vec![track],0)});
        let stopped=native.take("stop");local.finish(2,"idle");d.advance();
        assert_eq!(local.calls.lock().unwrap().as_slice(),["suspend","suspend"]);
        assert_eq!(d.applied,1);
        stopped.send(Ok(native_state("idle"))).unwrap();d.advance();
        assert_eq!(local.calls.lock().unwrap().as_slice(),["suspend","suspend","release","play"]);
        local.finish(4,"loading");d.advance();assert_eq!(d.applied,1);
        local.finish(4,"playing");d.advance();assert_eq!(d.applied,2);assert_eq!(d.source.as_deref(),Some("local"));
    }
    #[test]
    fn stop_supersedes_pending_apple_play_and_late_callback_cannot_restore_state() {
        let local=Arc::new(FakeLocal::default());let native=Arc::new(FakeNative::default());
        let mut d=Driver::new(local.clone(),native.clone());
        d.accept(Envelope{receipt:1,command:apple_play()});native.take("stop").send(Ok(native_state("idle"))).unwrap();local.finish(1,"idle");d.advance();
        let stale=native.take("play");
        d.accept(Envelope{receipt:2,command:Command::Stop});local.finish(2,"idle");d.advance();assert_eq!(d.applied,0);
        let stopped=native.take("stop");
        assert!(stale.send(Ok(native_state("playing"))).is_err(),"obsolete callback receiver must be discarded");
        stopped.send(Ok(native_state("idle"))).unwrap();d.advance();local.finish(4,"idle");d.advance();
        assert_eq!(d.applied,2);assert_eq!(d.snapshot()["transport"],"idle");assert_eq!(d.snapshot()["playing"],false);
    }
    #[test]
    fn settings_receipt_cannot_jump_over_unfinished_transport() {
        let local=Arc::new(FakeLocal::default());let native=Arc::new(FakeNative::default());
        let mut d=Driver::new(local.clone(),native.clone());
        d.accept(Envelope{receipt:1,command:apple_play()});
        d.accept(Envelope{receipt:2,command:Command::Settings(0.5,0.2,0.,true,true)});
        local.finish(2,"idle");d.advance();assert_eq!(d.applied,0);
        native.take("stop").send(Ok(native_state("idle"))).unwrap();d.advance();
        native.take("play").send(Ok(native_state("playing"))).unwrap();d.advance();assert_eq!(d.applied,2);
    }
}
