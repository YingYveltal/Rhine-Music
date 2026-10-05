use anyhow::{bail, Result};
use cookie_store::CookieStore;
use reqwest::{cookie::CookieStore as RequestCookies, header::HeaderValue, Url};
use serde_json::{json, Value};
use std::sync::Mutex;

#[derive(Default)]
pub(crate) struct SessionJar(Mutex<CookieStore>);

impl SessionJar {
    pub(crate) fn restore(session: Option<&Value>) -> Result<Self> {
        if let Some(cookies) = session.and_then(|value| value.get("cookieStore")) {
            let bytes = serde_json::to_vec(cookies)?;
            let store = cookie_store::serde::json::load(bytes.as_slice())
                .map_err(|_| anyhow::anyhow!("保存的 QQ 连接格式无效，请重新扫码"))?;
            return Ok(Self(Mutex::new(store)));
        }
        let jar = Self::default();
        if let Some(session) = session {
            let cookies = session["cookies"].as_array()
                .ok_or_else(|| anyhow::anyhow!("保存的 QQ 连接格式无效，请重新扫码"))?;
            for cookie in cookies {
                let domain = cookie["domain"].as_str().unwrap_or("").trim_start_matches('.');
                let Ok(url) = Url::parse(&format!("https://{domain}/")) else { continue };
                if !super::web::allowed(&url) && domain != "qq.com" { continue; }
                let name = cookie["name"].as_str().unwrap_or("");
                let value = cookie["value"].as_str().unwrap_or("");
                if name.is_empty() || name.contains([';', '\r', '\n']) || value.contains([';', '\r', '\n']) {
                    bail!("保存的 QQ 连接格式无效，请重新扫码");
                }
                jar.add(&format!("{name}={value}; Domain={domain}; Path=/; Secure; HttpOnly"), &url);
            }
        }
        Ok(jar)
    }

    pub(crate) fn add(&self, cookie: &str, url: &Url) {
        let _ = self.0.lock().unwrap().parse(cookie, url);
    }

    pub(crate) fn independent_copy(&self) -> Self {
        Self(Mutex::new(self.0.lock().unwrap().clone()))
    }

    pub(crate) fn export(&self) -> Value {
        let store = self.0.lock().unwrap();
        let cookies = store.iter_unexpired()
            .filter(|cookie| matches!(cookie.name(), "uin" | "qqmusic_uin" | "qqmusic_key" | "qm_keyst" | "p_skey" | "p_uin" | "skey"))
            .collect::<Vec<_>>();
        json!({"cookieStore": cookies})
    }
}

impl RequestCookies for SessionJar {
    fn set_cookies(&self, headers: &mut dyn Iterator<Item = &HeaderValue>, url: &Url) {
        for header in headers {
            if let Ok(cookie) = header.to_str() { self.add(cookie, url); }
        }
    }

    fn cookies(&self, url: &Url) -> Option<HeaderValue> {
        let store = self.0.lock().unwrap();
        let value = store.get_request_values(url)
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>().join("; ");
        if value.is_empty() { None } else { HeaderValue::from_str(&value).ok() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_preserves_domain_path_expiry_and_session_cookies() {
        let jar = SessionJar::default();
        let music = Url::parse("https://y.qq.com/").unwrap();
        jar.add("uin=123; Domain=qq.com; Path=/; Secure", &music);
        jar.add("qm_keyst=synthetic; Domain=qq.com; Path=/; Max-Age=600; Secure", &music);
        jar.add("qrsig=temporary; Path=/; Secure", &music);
        jar.add("skey=old; Domain=qq.com; Path=/; Expires=Sat, 01 Jan 2000 00:00:00 GMT", &music);
        let saved = jar.export();
        assert!(!saved.to_string().contains("temporary"));
        let restored = SessionJar::restore(Some(&saved)).unwrap();
        let gateway = Url::parse("https://u.y.qq.com/cgi-bin/musicu.fcg").unwrap();
        let cookie = restored.cookies(&gateway).unwrap();
        assert!(cookie.to_str().unwrap().contains("qm_keyst=synthetic"));
        assert!(!cookie.to_str().unwrap().contains("skey=old"));
        assert_eq!(saved["cookieStore"].as_array().unwrap().len(), 2);
        let mut expired = saved;
        for cookie in expired["cookieStore"].as_array_mut().unwrap() {
            cookie["expires"] = json!({"AtUtc":"2000-01-01T00:00:00Z"});
        }
        assert!(SessionJar::restore(Some(&expired)).unwrap().cookies(&music).is_none());
    }

    #[test]
    fn abandoned_response_cannot_mutate_another_cookie_store() {
        let jar = SessionJar::default();
        let copy = jar.independent_copy();
        let url = Url::parse("https://y.qq.com/").unwrap();
        copy.add("qm_keyst=synthetic; Domain=qq.com; Path=/", &url);
        assert!(jar.cookies(&url).is_none());
        assert!(copy.cookies(&url).is_some());
    }
}
