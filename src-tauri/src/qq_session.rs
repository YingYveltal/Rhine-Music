use anyhow::{bail, Result};
use rhine_qq_connector::web::WebClient;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{path::{Path, PathBuf}, sync::Mutex, time::{Duration, Instant}};

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct Credentials {
    pub session: Option<Value>,
    pub key: String,
}

pub(crate) trait CredentialStore: Send {
    fn load(&mut self) -> Result<Option<Credentials>>;
    fn save(&mut self, credentials: &Credentials) -> Result<()>;
    fn delete(&mut self) -> Result<()>;
}

pub(crate) struct Keychain;
const SERVICE: &str = "com.rhine.music.qq";
const ACCOUNT: &str = "connection";

impl CredentialStore for Keychain {
    fn load(&mut self) -> Result<Option<Credentials>> {
        use security_framework::passwords::{generic_password, PasswordOptions};
        match generic_password(PasswordOptions::new_generic_password(SERVICE, ACCOUNT)) {
            Ok(bytes) => serde_json::from_slice(&bytes).map(Some)
                .map_err(|_| anyhow::anyhow!("保存的连接格式无效，请重新扫码")),
            Err(error) if error.code() == -25300 => Ok(None),
            Err(_) => bail!("无法读取钥匙串，请检查系统授权后重试"),
        }
    }
    fn save(&mut self, credentials: &Credentials) -> Result<()> {
        security_framework::passwords::set_generic_password(SERVICE, ACCOUNT, &serde_json::to_vec(credentials)?)
            .map_err(|_| anyhow::anyhow!("无法保存钥匙串，请检查系统授权后重试"))
    }
    fn delete(&mut self) -> Result<()> {
        match security_framework::passwords::delete_generic_password(SERVICE, ACCOUNT) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == -25300 => Ok(()),
            Err(_) => bail!("本次连接已断开，但钥匙串尚未删除；请检查系统授权后再次忘记连接"),
        }
    }
}

struct Attempt {
    id: u64,
    started: Instant,
    web: Option<WebClient>,
    polling: bool,
}

struct State {
    web: WebClient,
    key: String,
    version: u64,
    account_version: u64,
    next_attempt: u64,
    pending: Option<Attempt>,
    verified: bool,
    validating: bool,
    remembered: bool,
    stored: bool,
    notice: Option<String>,
}

pub(crate) struct Connection {
    state: Mutex<State>,
    store: Mutex<Box<dyn CredentialStore>>,
    marker: PathBuf,
}

impl Connection {
    pub fn open(root: &Path, mut store: Box<dyn CredentialStore>, bootstrap: Option<Credentials>) -> Result<Self> {
        let marker = root.join("remember-connection");
        let marker_value = std::fs::read(&marker);
        let stored = marker_value.is_ok();
        let mut remembered = marker_value.as_ref().is_ok_and(|bytes| bytes == b"enabled");
        let mut notice = None;
        let credentials = if let Some(credentials) = bootstrap {
            if remembered { Self::write_marker(&marker, false)?; }
            remembered = false;
            credentials
        } else if remembered {
            match store.load() {
                Ok(Some(credentials)) => credentials,
                Ok(None) => { remembered = false; notice = Some("钥匙串中的连接已不存在，请重新扫码或填写 Key".into()); Credentials::default() }
                Err(error) => { remembered = false; notice = Some(error.to_string()); Credentials::default() }
            }
        } else { Credentials::default() };
        let web = match WebClient::new(credentials.session.as_ref()) {
            Ok(web) => web,
            Err(_) => {
                remembered = false;
                notice = Some("保存的登录格式无效，请重新扫码".into());
                WebClient::new(None)?
            }
        };
        let had_cookies = credentials.session.as_ref().is_some_and(|session| {
            session.get("cookieStore").or_else(||session.get("cookies"))
                .and_then(Value::as_array).is_some_and(|cookies|!cookies.is_empty())
        });
        if had_cookies && !web.authenticated() && notice.is_none() {
            notice = Some("保存的登录已过期或无法恢复，请重新扫码；已同步曲库仍保留".into());
        }
        Ok(Self {
            state: Mutex::new(State { web, key: credentials.key, version: 0, account_version: 0, next_attempt: 0,
                pending: None, verified: false, validating: false, remembered, stored, notice }),
            store: Mutex::new(store), marker,
        })
    }

    fn write_marker(marker: &Path, enabled: bool) -> Result<()> {
        let temporary = marker.with_extension("tmp");
        std::fs::write(&temporary, if enabled { b"enabled".as_slice() } else { b"disabled".as_slice() })
            .and_then(|_| std::fs::rename(&temporary, marker))
            .map_err(|_| anyhow::anyhow!("无法保存连接恢复设置，请检查应用数据目录权限"))
    }

    fn disable_restore(&self, state: &mut State) -> Result<()> {
        if state.stored || self.marker.exists() { Self::write_marker(&self.marker, false)?; }
        if state.remembered { state.notice = Some("连接已更新；如需重启后恢复，请重新保存到钥匙串".into()); }
        state.remembered = false;
        Ok(())
    }

    pub fn snapshot(&self) -> (u64, WebClient, String) {
        let state = self.state.lock().unwrap();
        (state.version, state.web.clone(), state.key.clone())
    }

    pub fn status(&self) -> Value {
        let state = self.state.lock().unwrap();
        let available = state.web.authenticated();
        let connection_state = if state.validating { "checking" } else if available && state.verified { "connected" }
            else if available { "unverified" } else if state.verified { "expired" } else { "disconnected" };
        json!({"connected":available && state.verified,"connectionState":connection_state,
            "officialConfigured":!state.key.is_empty(),"remembered":state.remembered,
            "storedConnection":state.stored,"connectionNotice":state.notice,"loginPending":state.pending.is_some()})
    }

    pub fn account_snapshot(&self) -> (u64, WebClient) {
        let state = self.state.lock().unwrap();
        (state.account_version, state.web.clone())
    }

    pub fn if_account_current<T>(&self, version: u64, work: impl FnOnce() -> Result<T>) -> Result<T> {
        let state = self.state.lock().unwrap();
        if state.account_version != version { bail!("账户已改变，本次同步已取消"); }
        work()
    }

    pub fn if_current<T>(&self, version: u64, work: impl FnOnce() -> Result<T>) -> Result<T> {
        let state = self.state.lock().unwrap();
        if state.version != version { bail!("连接已改变，本次请求已取消"); }
        work()
    }

    pub fn begin_login(&self) -> u64 {
        let mut state = self.state.lock().unwrap();
        state.next_attempt += 1;
        let id = state.next_attempt;
        state.pending = Some(Attempt { id, started: Instant::now(), web: None, polling: false });
        id
    }

    pub fn cancel_login(&self, id: u64) {
        let mut state = self.state.lock().unwrap();
        if state.pending.as_ref().is_some_and(|attempt| attempt.id == id) { state.pending = None; }
    }

    fn pending(state: &mut State, id: u64) -> Result<&mut Attempt> {
        if state.pending.as_ref().is_some_and(|attempt| attempt.id == id && attempt.started.elapsed() >= Duration::from_secs(180)) {
            state.pending = None;
            bail!("二维码已过期，请重新获取");
        }
        state.pending.as_mut().filter(|attempt| attempt.id == id).ok_or_else(|| anyhow::anyhow!("登录请求已取消"))
    }

    pub fn check_attempt(&self, id: u64) -> Result<()> {
        Self::pending(&mut self.state.lock().unwrap(), id).map(|_| ())
    }

    pub fn accept_qr(&self, id: u64, web: WebClient) -> Result<()> {
        Self::pending(&mut self.state.lock().unwrap(), id)?.web = Some(web);
        Ok(())
    }

    pub fn polling_copy(&self, id: u64) -> Result<WebClient> {
        let mut state = self.state.lock().unwrap();
        let attempt = Self::pending(&mut state, id)?;
        if attempt.polling { bail!("正在检查二维码，请稍候"); }
        let web = attempt.web.as_ref().ok_or_else(|| anyhow::anyhow!("二维码尚未准备好"))?.independent_copy()?;
        attempt.polling = true;
        Ok(web)
    }

    pub fn accept_poll(&self, id: u64, web: WebClient, result: &Value, on_connected: impl FnOnce()) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        let attempt = Self::pending(&mut state, id)?;
        attempt.polling = false;
        if result["connected"] == true {
            if !web.authenticated() { bail!("未取得有效登录，请重新扫码"); }
            self.disable_restore(&mut state)?;
            state.web = web;
            state.verified = true;
            state.validating = false;
            state.version += 1;
            state.account_version += 1;
            state.pending = None;
            on_connected();
        } else if result["expired"] == true || result["cancelled"] == true {
            state.pending = None;
        } else { attempt.web = Some(web); }
        Ok(())
    }

    pub fn set_key(&self, version: u64, key: String) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        if state.version != version { bail!("连接已改变，请重新提交 Key"); }
        if state.key != key {
            self.disable_restore(&mut state)?;
            state.key = key;
            state.version += 1;
            state.validating = false;
        }
        Ok(())
    }

    pub fn begin_validation(&self) -> Result<Option<(u64, WebClient)>> {
        let mut state = self.state.lock().unwrap();
        if state.validating || !state.web.authenticated() { return Ok(None); }
        let web = state.web.independent_copy()?;
        state.validating = true;
        Ok(Some((state.version, web)))
    }

    pub fn finish_validation(&self, version: u64, web: WebClient, result: Result<()>) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        if state.version != version { bail!("连接已改变，本次检查已取消"); }
        state.validating = false;
        match result {
            Ok(()) => { state.web = web; state.verified = true; state.notice = None; Ok(()) }
            Err(error) => {
                state.verified = false;
                state.notice = Some(format!("连接未通过验证：{error}。可重试验证或重新扫码，已同步曲库仍保留"));
                Err(error)
            }
        }
    }

    pub fn remember(&self) -> Result<()> {
        let mut store = self.store.lock().unwrap();
        let (version, credentials) = {
            let mut state = self.state.lock().unwrap();
            if state.pending.is_some() || state.validating { bail!("请先完成登录或连接验证，再保存连接"); }
            if state.web.authenticated() && !state.verified { bail!("请先验证连接，再保存到钥匙串"); }
            if !state.web.authenticated() && state.key.is_empty() { bail!("请先登录或填写有效的官方 Key"); }
            self.disable_restore(&mut state)?;
            Self::write_marker(&self.marker, false)?;
            state.stored = true;
            (state.version, Credentials { session: Some(state.web.export_session()), key: state.key.clone() })
        };
        store.save(&credentials)?;
        let mut state = self.state.lock().unwrap();
        state.stored = true;
        if state.version != version { bail!("连接已改变，自动恢复未启用；请重新保存当前连接"); }
        Self::write_marker(&self.marker, true)?;
        state.remembered = true;
        state.notice = None;
        Ok(())
    }

    pub fn disconnect(&self, clear_library: impl FnOnce() -> Result<()>) -> Result<()> {
        let (version, marker_result) = self.clear_connection(clear_library)?;
        self.forget_saved(version, marker_result)
    }

    fn clear_connection(&self, clear_library: impl FnOnce() -> Result<()>) -> Result<(u64, Result<()>)> {
        let empty = WebClient::new(None)?;
        let result = {
            let mut state = self.state.lock().unwrap();
            let marker_result = self.disable_restore(&mut state);
            state.web = empty; state.key.clear(); state.pending = None;
            state.version += 1; state.verified = false; state.validating = false;
            state.account_version += 1;
            state.remembered = false; state.notice = None;
            (state.version, marker_result.and(clear_library()))
        };
        Ok(result)
    }

    fn forget_saved(&self, version: u64, marker_result: Result<()>) -> Result<()> {
        let mut store = self.store.lock().unwrap();
        let stored = {
            let state = self.state.lock().unwrap();
            // A newer save may have acquired the store first while disconnect
            // was waiting. Check again while holding the store, before delete.
            if state.version != version { bail!("连接已改变，已跳过旧连接删除"); }
            state.stored
        };
        let deleted = if stored { store.delete() } else { Ok(()) };
        let mut state = self.state.lock().unwrap();
        if deleted.is_ok() { state.stored = false; }
        if let Err(error) = marker_result.and(deleted) {
            if state.version == version { state.notice = Some(error.to_string()); }
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, mpsc};

    #[derive(Default)]
    struct Memory {
        bytes: Option<Vec<u8>>, loads: usize, saves: usize, deletes: usize,
        fail_load: bool, fail_save: bool, fail_delete: bool,
        save_gate: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
    }
    struct FakeStore(Arc<Mutex<Memory>>);
    impl CredentialStore for FakeStore {
        fn load(&mut self) -> Result<Option<Credentials>> {
            let mut memory = self.0.lock().unwrap(); memory.loads += 1;
            if memory.fail_load { bail!("模拟读取被取消"); }
            memory.bytes.as_ref().map(|bytes| serde_json::from_slice(bytes)
                .map_err(|_| anyhow::anyhow!("模拟保存格式无效"))).transpose()
        }
        fn save(&mut self, credentials: &Credentials) -> Result<()> {
            let gate = { let mut memory = self.0.lock().unwrap(); memory.saves += 1;
                if memory.fail_save { bail!("模拟保存被取消"); }
                memory.save_gate.take()
            };
            if let Some((entered, resume)) = gate {
                entered.send(()).unwrap(); resume.recv_timeout(Duration::from_secs(3)).unwrap();
            }
            self.0.lock().unwrap().bytes = Some(serde_json::to_vec(credentials)?); Ok(())
        }
        fn delete(&mut self) -> Result<()> {
            let mut memory = self.0.lock().unwrap(); memory.deletes += 1;
            if memory.fail_delete { bail!("模拟删除被取消"); }
            memory.bytes = None; Ok(())
        }
    }
    fn credentials(uin: &str) -> Credentials {
        Credentials { key: "synthetic-api-secret".into(), session: Some(json!({"cookies":[
            {"domain":".qq.com","name":"uin","value":uin},
            {"domain":".qq.com","name":"qm_keyst","value":"synthetic-cookie-secret"}
        ]})) }
    }
    fn web(uin: &str) -> WebClient { WebClient::new(credentials(uin).session.as_ref()).unwrap() }
    fn open(root: &Path, memory: &Arc<Mutex<Memory>>, bootstrap: Option<Credentials>) -> Connection {
        Connection::open(root, Box::new(FakeStore(memory.clone())), bootstrap).unwrap()
    }
    fn verified(connection: &Connection) {
        let (version, web) = connection.begin_validation().unwrap().unwrap();
        connection.finish_validation(version, web, Ok(())).unwrap();
    }
    fn setup() -> (tempfile::TempDir, Arc<Mutex<Memory>>, Connection) {
        let dir = tempfile::tempdir().unwrap(); let memory = Arc::new(Mutex::new(Memory::default()));
        let connection = open(dir.path(), &memory, Some(credentials("10001"))); verified(&connection);
        (dir, memory, connection)
    }

    #[test]
    fn cancelled_expired_failed_and_replaced_attempts_preserve_current_account() {
        let (_dir, _memory, connection) = setup();
        let first = connection.begin_login();
        connection.accept_qr(first, WebClient::new(None).unwrap()).unwrap();
        let _in_flight = connection.polling_copy(first).unwrap();
        assert!(connection.polling_copy(first).is_err());
        connection.cancel_login(first);
        assert!(connection.accept_poll(first, web("20002"), &json!({"connected":true}), || panic!("late commit")).is_err());
        assert_eq!(connection.snapshot().1.uin(), "10001");
        for terminal in [json!({"expired":true}), json!({"cancelled":true})] {
            let id = connection.begin_login();
            connection.accept_poll(id, WebClient::new(None).unwrap(), &terminal, || panic!("terminal commit")).unwrap();
            assert_eq!(connection.status()["loginPending"], false);
            assert_eq!(connection.status()["connected"], true);
        }
        let timed_out = connection.begin_login();
        connection.state.lock().unwrap().pending.as_mut().unwrap().started = Instant::now() - Duration::from_secs(181);
        assert!(connection.accept_poll(timed_out, web("20002"), &json!({"connected":true}), || panic!("expired commit")).is_err());
        let failed = connection.begin_login();
        assert!(connection.accept_poll(failed, WebClient::new(None).unwrap(), &json!({"connected":true}), || panic!("invalid commit")).is_err());
        connection.cancel_login(failed); // request() also cancels on a transport/protocol error.
        let old = connection.begin_login(); let fresh = connection.begin_login();
        connection.cancel_login(old);
        assert!(connection.accept_qr(old, web("30003")).is_err());
        connection.accept_poll(fresh, web("20002"), &json!({"connected":true}), || {}).unwrap();
        assert!(connection.accept_poll(old, web("30003"), &json!({"connected":true}), || panic!("stale commit")).is_err());
        assert_eq!(connection.snapshot().1.uin(), "20002");
    }

    #[test]
    fn save_restart_change_and_unsaved_restart_have_explicit_policy() {
        let (dir, memory, connection) = setup();
        let unsaved = open(dir.path(), &memory, None);
        assert_eq!(unsaved.status()["connected"], false); assert_eq!(memory.lock().unwrap().loads, 0);
        connection.remember().unwrap();
        assert_eq!(connection.status()["remembered"], true);
        let restored = open(dir.path(), &memory, None);
        assert_eq!(restored.status()["connectionState"], "unverified");
        assert_eq!(restored.status()["remembered"], true);
        verified(&restored); assert_eq!(restored.status()["connected"], true);
        let (account_version, _) = restored.account_snapshot();
        restored.set_key(restored.snapshot().0, "synthetic-new-key".into()).unwrap();
        assert!(restored.if_account_current(account_version, || Ok(())).is_ok());
        assert_eq!(restored.status()["remembered"], false);
        assert_eq!(open(dir.path(), &memory, None).status()["officialConfigured"], false);
        restored.remember().unwrap();
        let id = restored.begin_login();
        assert!(restored.remember().is_err());
        restored.accept_poll(id, web("20002"), &json!({"connected":true}), || {}).unwrap();
        assert!(restored.if_account_current(account_version, || Ok(())).is_err());
        assert_eq!(restored.status()["remembered"], false);
        assert_eq!(open(dir.path(), &memory, None).status()["connectionState"], "disconnected");
    }

    #[test]
    fn save_load_and_delete_failures_do_not_claim_saved_or_connected() {
        let (dir, memory, connection) = setup();
        connection.remember().unwrap();
        memory.lock().unwrap().fail_save = true;
        assert!(connection.remember().is_err());
        assert_eq!(connection.status()["remembered"], false);
        assert_eq!(connection.status()["connected"], true);
        assert_eq!(open(dir.path(), &memory, None).status()["connected"], false);
        memory.lock().unwrap().fail_save = false; connection.remember().unwrap();
        memory.lock().unwrap().fail_load = true;
        let denied = open(dir.path(), &memory, None);
        assert_eq!(denied.status()["remembered"], false);
        assert!(denied.status()["connectionNotice"].as_str().unwrap().contains("取消"));
        memory.lock().unwrap().fail_load = false;
        memory.lock().unwrap().bytes = Some(b"invalid synthetic data".to_vec());
        assert_eq!(open(dir.path(), &memory, None).status()["remembered"], false);
        memory.lock().unwrap().bytes = None;
        assert!(open(dir.path(), &memory, None).status()["connectionNotice"].as_str().unwrap().contains("不存在"));
        connection.remember().unwrap(); memory.lock().unwrap().fail_delete = true;
        let pending = connection.begin_login(); let mut cleared = false;
        assert!(connection.disconnect(|| { cleared = true; Ok(()) }).is_err()); assert!(cleared);
        assert_eq!(connection.status()["connected"], false);
        assert_eq!(connection.status()["remembered"], false);
        assert_eq!(connection.status()["storedConnection"], true);
        assert!(connection.accept_poll(pending, web("20002"), &json!({"connected":true}), || panic!("late commit")).is_err());
        assert_eq!(open(dir.path(), &memory, None).status()["officialConfigured"], false);
        memory.lock().unwrap().fail_delete = false;
        connection.disconnect(|| Ok(())).unwrap(); connection.disconnect(|| Ok(())).unwrap();
        assert_eq!(connection.status()["storedConnection"], false);
        assert!(memory.lock().unwrap().bytes.is_none());
    }

    #[test]
    fn marker_write_failure_does_not_claim_saved() {
        let (dir, memory, connection) = setup();
        std::fs::create_dir(dir.path().join("remember-connection.tmp")).unwrap();
        assert!(connection.remember().is_err());
        assert_eq!(connection.status()["remembered"], false);
        assert_eq!(memory.lock().unwrap().saves, 0);
    }

    #[test]
    fn saved_cookie_expiry_survives_fake_storage_and_restart_without_secrets_in_status() {
        let (dir, memory, connection) = setup();
        connection.remember().unwrap();
        let mut saved: Credentials = serde_json::from_slice(memory.lock().unwrap().bytes.as_ref().unwrap()).unwrap();
        let cookies = saved.session.as_mut().unwrap()["cookieStore"].as_array_mut().unwrap();
        for cookie in cookies { cookie["expires"] = json!({"AtUtc":"2000-01-01T00:00:00Z"}); }
        memory.lock().unwrap().bytes = Some(serde_json::to_vec(&saved).unwrap());
        let expired = open(dir.path(), &memory, None);
        assert_eq!(expired.status()["connected"], false);
        assert!(expired.status()["connectionNotice"].as_str().unwrap().contains("过期"));
        assert!(expired.begin_validation().unwrap().is_none());
        for bytes in [connection.status().to_string(), expired.status().to_string(), std::fs::read_to_string(dir.path().join("remember-connection")).unwrap()] {
            assert!(!bytes.contains("synthetic-api-secret")); assert!(!bytes.contains("synthetic-cookie-secret"));
        }
        let only_key = open(dir.path(), &memory, Some(Credentials { key:"synthetic-key-only".into(), session:None }));
        only_key.remember().unwrap(); let restored = open(dir.path(), &memory, None);
        assert_eq!(restored.status()["officialConfigured"], true);
        assert!(restored.status()["connectionNotice"].is_null());
    }

    #[test]
    fn validation_retry_and_late_callbacks_cannot_override_a_new_connection() {
        let (_dir, _memory, connection) = setup();
        let (version, copy) = connection.begin_validation().unwrap().unwrap();
        assert!(connection.begin_validation().unwrap().is_none());
        assert!(connection.finish_validation(version, copy, Err(anyhow::anyhow!("模拟网络超时"))).is_err());
        assert_eq!(connection.status()["connectionState"], "unverified");
        assert!(connection.remember().is_err());
        verified(&connection);
        let (version, copy) = connection.begin_validation().unwrap().unwrap();
        let id = connection.begin_login();
        connection.accept_poll(id, web("20002"), &json!({"connected":true}), || {}).unwrap();
        assert!(connection.finish_validation(version, copy, Ok(())).is_err());
        assert_eq!(connection.snapshot().1.uin(), "20002");
        let version = connection.snapshot().0; let account_version = connection.account_snapshot().0;
        connection.disconnect(|| Ok(())).unwrap();
        assert!(connection.if_current::<()>(version, || panic!("late search commit")).is_err());
        assert!(connection.if_account_current::<()>(account_version, || panic!("late sync commit")).is_err());
    }

    #[test]
    fn save_finishing_after_key_change_cannot_enable_old_credentials() {
        let (dir, memory, connection) = setup(); let connection = Arc::new(connection);
        let (entered_tx, entered_rx) = mpsc::channel(); let (resume_tx, resume_rx) = mpsc::channel();
        memory.lock().unwrap().save_gate = Some((entered_tx, resume_rx));
        let worker = { let connection = connection.clone(); std::thread::spawn(move || connection.remember()) };
        entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        connection.set_key(connection.snapshot().0, "synthetic-new-key".into()).unwrap();
        resume_tx.send(()).unwrap(); assert!(worker.join().unwrap().is_err());
        assert_eq!(connection.status()["remembered"], false);
        assert_eq!(open(dir.path(), &memory, None).status()["officialConfigured"], false);
        connection.remember().unwrap(); assert_eq!(connection.status()["remembered"], true);
    }

    #[test]
    fn logout_invalidates_connection_before_waiting_for_an_inflight_save() {
        let (dir, memory, connection) = setup(); let connection = Arc::new(connection);
        let (entered_tx, entered_rx) = mpsc::channel(); let (resume_tx, resume_rx) = mpsc::channel();
        memory.lock().unwrap().save_gate = Some((entered_tx, resume_rx));
        let saving = { let connection = connection.clone(); std::thread::spawn(move || connection.remember()) };
        entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        let (cleared_tx, cleared_rx) = mpsc::channel();
        let logout = { let connection = connection.clone(); std::thread::spawn(move || connection.disconnect(|| { cleared_tx.send(()).unwrap(); Ok(()) })) };
        cleared_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(connection.status()["connected"], false);
        resume_tx.send(()).unwrap(); assert!(saving.join().unwrap().is_err()); logout.join().unwrap().unwrap();
        assert!(memory.lock().unwrap().bytes.is_none());
        assert_eq!(open(dir.path(), &memory, None).status()["officialConfigured"], false);
    }

    #[test]
    fn delayed_forget_does_not_delete_a_newer_saved_connection() {
        let (dir, memory, connection) = setup(); let connection = Arc::new(connection);
        let (entered_tx, entered_rx) = mpsc::channel(); let (resume_tx, resume_rx) = mpsc::channel();
        memory.lock().unwrap().save_gate = Some((entered_tx, resume_rx));
        let saving_a = { let connection = connection.clone(); std::thread::spawn(move || connection.remember()) };
        entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();

        // Pause disconnect at the real boundary between invalidating A and
        // acquiring the store lock, then deterministically let B save first.
        let (old_version, marker_result) = connection.clear_connection(|| Ok(())).unwrap();
        let id = connection.begin_login();
        connection.accept_poll(id, web("20002"), &json!({"connected":true}), || {}).unwrap();
        resume_tx.send(()).unwrap(); assert!(saving_a.join().unwrap().is_err());
        connection.remember().unwrap();
        let deletes_before = memory.lock().unwrap().deletes;

        let delayed_result = connection.forget_saved(old_version, marker_result);
        assert_eq!(memory.lock().unwrap().deletes, deletes_before, "late disconnect must not delete B");
        assert!(delayed_result.is_err());
        assert_eq!(connection.status()["remembered"], true);
        let restored = open(dir.path(), &memory, None);
        assert_eq!(restored.snapshot().1.uin(), "20002");
        assert_eq!(restored.status()["remembered"], true);

        connection.disconnect(|| Ok(())).unwrap();
        assert!(memory.lock().unwrap().bytes.is_none());
        assert_eq!(connection.status()["remembered"], false);
        assert_eq!(open(dir.path(), &memory, None).status()["officialConfigured"], false);
    }
}
