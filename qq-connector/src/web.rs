//! Authenticated QQ web protocol; distinct from the official discovery API.
//! Only fixed Tencent endpoints are used, and redirect credentials stay native.
use anyhow::{bail, Context, Result};
use base64::Engine;
use reqwest::{blocking::Client, cookie::{CookieStore,Jar}, Url};
use serde_json::{json,Value};
use std::{sync::Arc,time::{Duration,SystemTime,UNIX_EPOCH},io::Read};

const GATEWAY:&str="https://u.y.qq.com/cgi-bin/musicu.fcg";
const REFERER:&str="https://y.qq.com/";
fn millis()->u128 {SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()}
pub fn hash33(value:&str,seed:u32)->u32 {value.chars().fold(seed,|a,c|a.wrapping_mul(33).wrapping_add(c as u32))&0x7fffffff}
fn allowed(url:&Url)->bool {url.scheme()=="https" && url.username().is_empty() && url.password().is_none() && url.port().is_none_or(|p|p==443) && matches!(url.host_str(),Some("u.y.qq.com"|"y.qq.com"|"c.y.qq.com"|"c6.y.qq.com"|"ssl.ptlogin2.qq.com"|"ptlogin2.qq.com"|"graph.qq.com"|"ssl.ptlogin2.graph.qq.com"))}

#[derive(Clone)]
pub struct WebClient { http:Client, jar:Arc<Jar>, qrsig:String, qr_started:u128 }
impl WebClient {
    pub fn new(session:Option<&Value>)->Result<Self> {
        let jar=Arc::new(Jar::default());
        if let Some(cookies)=session.and_then(|s|s["cookies"].as_array()) {
            for cookie in cookies {
                let domain=cookie["domain"].as_str().unwrap_or("").trim_start_matches('.');
                let Ok(url)=Url::parse(&format!("https://{domain}/")) else {continue};
                if !allowed(&url) && domain!="qq.com" {continue}
                let name=cookie["name"].as_str().unwrap_or("");let value=cookie["value"].as_str().unwrap_or("");
                if name.contains([';','\r','\n'])||value.contains([';','\r','\n']) {continue}
                jar.add_cookie_str(&format!("{name}={value}; Domain={domain}; Path=/; Secure; HttpOnly"),&url);
            }
        }
        let http=Client::builder().cookie_provider(jar.clone()).redirect(reqwest::redirect::Policy::none())
            .user_agent("Mozilla/5.0").connect_timeout(Duration::from_secs(8)).timeout(Duration::from_secs(20)).build()?;
        Ok(Self{http,jar,qrsig:String::new(),qr_started:0})
    }
    fn cookie(&self,name:&str,host:&str)->String {
        self.jar.cookies(&Url::parse(&format!("https://{host}/")).unwrap()).and_then(|v|v.to_str().ok().map(str::to_owned))
            .unwrap_or_default().split(';').filter_map(|p|p.trim().split_once('='))
            .find(|(n,_)|*n==name).map(|(_,v)|v.to_owned()).unwrap_or_default()
    }
    pub fn export_session(&self)->Value {
        let mut cookies=Vec::new();
        for host in ["y.qq.com","graph.qq.com"] {
            for name in ["uin","qqmusic_uin","qqmusic_key","qm_keyst","p_skey","p_uin","skey"] {
                let value=self.cookie(name,host);if !value.is_empty(){cookies.push(json!({"domain":host,"name":name,"value":value}))}
            }
        }
        json!({"cookies":cookies})
    }
    pub fn uin(&self)->String {let s=self.cookie("uin","y.qq.com");if s.is_empty(){self.cookie("qqmusic_uin","y.qq.com")}else{s}}
    pub fn authenticated(&self)->bool { !self.uin().is_empty() && !self.auth().is_empty() }
    fn auth(&self)->String {let s=self.cookie("qm_keyst","y.qq.com");if s.is_empty(){self.cookie("qqmusic_key","y.qq.com")}else{s}}
    fn comm(&self)->Value {let key=self.auth();let gtk=hash33(&key,5381);json!({"ct":24,"cv":4747474,"platform":"yqq.json","format":"json","uin":self.uin(),"authst":key,"g_tk":gtk,"g_tk_new_20200303":gtk,"need_new_code":1})}
    fn checked(&self,url:&str)->Result<reqwest::blocking::RequestBuilder> {
        let parsed=Url::parse(url).map_err(|_|anyhow::anyhow!("QQ 地址无效"))?;if !allowed(&parsed){bail!("QQ 返回了不支持的地址")}
        Ok(self.http.get(parsed).header("Referer",REFERER).header("Origin","https://y.qq.com"))
    }
    fn data(response:reqwest::blocking::Response)->Result<Value>{
        if !response.status().is_success(){bail!("QQ 服务 HTTP {}",response.status().as_u16())}
        let mut bytes=Vec::new();response.take(4*1024*1024+1).read_to_end(&mut bytes).map_err(|_|anyhow::anyhow!("QQ 响应读取失败"))?;
        if bytes.len()>4*1024*1024 {bail!("QQ 响应过大")}
        serde_json::from_slice(&bytes).map_err(|_|anyhow::anyhow!("QQ 响应格式变化"))
    }
    fn get_json(&self,url:&str,params:Value)->Result<Value>{
        let q=params.as_object().context("参数无效")?.iter().map(|(k,v)|(k.clone(),v.as_str().map(str::to_owned).unwrap_or_else(||v.to_string()))).collect::<Vec<_>>();
        Self::data(self.checked(url)?.query(&q).send().map_err(|_|anyhow::anyhow!("QQ 网络连接失败，请重试"))?)
    }
    pub fn rpc(&self,module:&str,method:&str,param:Value,authenticated:bool)->Result<Value>{
        let mut body=json!({module:{"module":module,"method":method,"param":param}});if authenticated{body["comm"]=self.comm()}
        let data=Self::data(self.http.post(GATEWAY).header("Referer",REFERER).json(&body).send().map_err(|_|anyhow::anyhow!("QQ 网络连接失败，请重试"))?)?;
        if data["code"]!=0||data[module]["code"]!=0{bail!("QQ 请求未通过（{}/{}），请检查登录状态或稍后重试",data["code"],data[module]["code"])}
        Ok(data[module]["data"].clone())
    }
    pub fn qr(&mut self)->Result<String>{
        *self=Self::new(None)?;
        let response=self.checked("https://ssl.ptlogin2.qq.com/ptqrshow")?.query(&[("appid","716027609"),("e","2"),("l","M"),("s","3"),("d","72"),("v","4"),("daid","383"),("pt_3rd_aid","100497308"),("u1","https://graph.qq.com/oauth2.0/login_jump")]).send().map_err(|_|anyhow::anyhow!("二维码获取失败"))?;
        self.qrsig=self.cookie("qrsig","ssl.ptlogin2.qq.com");let bytes=response.bytes().map_err(|_|anyhow::anyhow!("二维码读取失败"))?;
        if self.qrsig.is_empty()||!bytes.starts_with(b"\x89PNG"){bail!("没有取得有效二维码")}
        self.qr_started=millis();Ok(format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes)))
    }
    pub fn poll(&mut self)->Result<Value>{
        if self.authenticated(){return Ok(json!({"connected":true,"message":"QQ 音乐已连接"}))}
        if self.qrsig.is_empty()||millis()-self.qr_started>180000{return Ok(json!({"expired":true,"message":"二维码已过期，请重新获取"}))}
        let q=json!({"u1":"https://graph.qq.com/oauth2.0/login_jump","ptqrtoken":hash33(&self.qrsig,0),"ptredirect":"0","h":"1","t":"1","g":"1","from_ui":"1","ptlang":"2052","action":format!("0-0-{}",millis()),"js_ver":"20102616","js_type":"1","login_sig":"","pt_uistyle":"40","aid":"716027609","daid":"383","pt_3rd_aid":"100497308"});
        let params=q.as_object().unwrap().iter().map(|(k,v)|(k.clone(),v.as_str().map(str::to_owned).unwrap_or_else(||v.to_string()))).collect::<Vec<_>>();
        let text=self.checked("https://ssl.ptlogin2.qq.com/ptqrlogin")?.query(&params).send().map_err(|_|anyhow::anyhow!("二维码检查失败"))?.text().map_err(|_|anyhow::anyhow!("登录响应读取失败"))?;
        let re=regex::Regex::new("'([^']*)'").unwrap();let fields=re.captures_iter(&text).map(|c|c[1].to_owned()).collect::<Vec<_>>();
        match fields.first().map(String::as_str){
            Some("66")=>return Ok(json!({"message":"请用手机 QQ 扫码"})),
            Some("67")=>return Ok(json!({"message":"请在手机 QQ 上确认登录"})),
            Some("65"|"68")=>return Ok(json!({"expired":true,"message":"二维码已过期或登录取消"})),
            Some("0")=>{},_=>bail!("QQ 登录检查未通过"),
        }
        self.checked(fields.get(2).context("登录回调缺失")?)?.send().map_err(|_|anyhow::anyhow!("QQ 登录跳转失败"))?;
        self.finish_oauth()
    }
    fn finish_oauth(&mut self)->Result<Value>{
        let p_skey=self.cookie("p_skey","graph.qq.com");if p_skey.is_empty(){bail!("未取得 QQ 授权票据，请重新扫码")}
        let response=self.http.post("https://graph.qq.com/oauth2.0/authorize").header("Referer",REFERER).header("Origin","https://y.qq.com").form(&json!({"response_type":"code","client_id":"100497308","redirect_uri":"https://y.qq.com/portal/wx_redirect.html?login_type=1&surl=https://y.qq.com/","scope":"get_user_info","state":"state","switch":"","from_ptlogin":"1","src":"1","update_auth":"1","openapi":"1010","g_tk":hash33(&p_skey,5381),"auth_time":chrono::Local::now().format("%a %b %e %H:%M:%S %Y").to_string(),"ui":uuid::Uuid::new_v4().to_string().to_uppercase()})).send().map_err(|_|anyhow::anyhow!("QQ 音乐授权失败"))?;
        let location=response.headers().get("Location").and_then(|h|h.to_str().ok()).context("QQ 需要额外网页授权")?;
        let location=Url::parse(location).map_err(|_|anyhow::anyhow!("授权跳转无效"))?;
        if !allowed(&location)||location.host_str()!=Some("y.qq.com"){bail!("QQ 授权未完成，请刷新二维码并重新扫码确认")}
        let code=location.query_pairs().find(|(k,_)|k=="code").map(|(_,v)|v.into_owned()).context("没有取得 QQ 音乐授权码")?;
        let data=Self::data(self.http.post(GATEWAY).header("Referer",REFERER).json(&json!({"comm":{"g_tk":hash33(&p_skey,5381),"platform":"yqq","ct":24,"cv":0},"req":{"module":"QQConnectLogin.LoginServer","method":"QQLogin","param":{"code":code}}})).send().map_err(|_|anyhow::anyhow!("QQ 音乐登录失败"))?)?;
        if data["code"]!=0||data["req"]["code"]!=0{bail!("QQ 音乐未接受本次授权")}
        if !self.authenticated(){bail!("授权完成但会话未建立，请重新扫码")}
        self.qrsig.clear();Ok(json!({"connected":true,"message":"QQ 音乐已连接"}))
    }
    pub fn directory(&self)->Result<Value>{
        if !self.authenticated(){bail!("请先用 QQ 扫码连接账户")}
        let data=self.get_json("https://c6.y.qq.com/rsc/fcgi-bin/fcg_get_profile_homepage.fcg",json!({"cv":4747474,"ct":24,"format":"json","cid":205360838,"userid":self.uin(),"loginUin":self.uin(),"uin":self.uin(),"reqfrom":1,"reqtype":0,"g_tk":hash33(&self.auth(),5381),"g_tk_new_20200303":hash33(&self.auth(),5381)}))?;
        if data["code"]!=0 {bail!("曲库读取失败，登录可能已过期，请重新扫码")}
        if !data["data"]["mydiss"]["list"].is_array(){bail!("QQ 未返回个人歌单目录")}
        Ok(data["data"].clone())
    }
    pub fn playlist(&self,id:u64,liked:bool)->Result<Value>{
        let mut tracks=Vec::new();let mut expected=None;let mut name=String::new();let mut cover=String::new();
        for _ in 0..100 {
            let data=self.rpc("music.srfDissInfo.DissInfo","CgiGetDiss",json!({"disstid":id,"dirid":if liked{201}else{0},"tag":true,"song_begin":tracks.len(),"song_num":100,"userinfo":true,"orderlist":true,"onlysonglist":false}),true)?;
            let batch=data["songlist"].as_array().context("歌单列表缺失")?;let total=data["total_song_num"].as_u64().context("歌单总数缺失")? as usize;
            if expected.is_some_and(|n|n!=total){bail!("歌单读取期间发生变化，请重新同步")};expected=Some(total);
            if !batch.is_empty()&&tracks.len()>=batch.len()&&tracks[tracks.len()-batch.len()..]==batch[..]{bail!("歌单分页没有前进")}
            if name.is_empty(){name=data["dirinfo"]["title"].as_str().unwrap_or("QQ 歌单").into();cover=data["dirinfo"]["picurl"].as_str().unwrap_or("").into()}
            tracks.extend(batch.iter().cloned());
            if tracks.len()>=total ||batch.is_empty(){if tracks.len()!=total{bail!("歌单未取全（{}/{total}），原曲库已保留",tracks.len())};return Ok(json!({"title":name,"cover":cover,"tracks":tracks}))}
            std::thread::sleep(Duration::from_millis(300));
        }
        bail!("歌单超过本次分页上限")
    }
    pub fn collections(&self,albums:bool)->Result<Vec<Value>>{
        let mut out=Vec::new();for page in 0..50{
            let d=self.get_json("https://c.y.qq.com/fav/fcgi-bin/fcg_get_profile_order_asset.fcg",json!({"ct":20,"cid":205360956,"userid":self.uin(),"reqtype":if albums{2}else{3},"sin":page*20,"ein":(page+1)*20-if albums{1}else{0},"format":"json","g_tk":hash33(&self.auth(),5381)}))?;
            if d["code"]!=0||d["subcode"].as_i64().unwrap_or(0)!=0{bail!("收藏读取失败")}
            let data=&d["data"];let list=data[if albums{"albumlist"}else{"cdlist"}].as_array().context("收藏列表缺失")?;out.extend(list.iter().cloned());
            if data["has_more"]==0{if data[if albums{"totalalbum"}else{"totaldiss"}].as_u64()!=Some(out.len() as u64){bail!("收藏目录没有读取完整")};return Ok(out)}
            if list.is_empty(){bail!("收藏分页未前进")}
        }bail!("收藏目录超过本次分页上限")
    }
    pub fn album(&self,mid:&str)->Result<Vec<Value>>{
        let mut tracks=Vec::new();for _ in 0..40 {
            let d=self.rpc("music.musichallAlbum.AlbumSongList","GetAlbumSongList",json!({"albumMid":mid,"begin":tracks.len(),"num":100,"order":2}),true)?;
            let items=d["songList"].as_array().context("专辑曲目列表缺失")?;
            for t in items {tracks.push(if t["songInfo"].is_object(){t["songInfo"].clone()}else{t.clone()})}
            let total=d["totalNum"].as_u64().context("专辑曲目总数缺失")? as usize;
            if tracks.len()>=total {return Ok(tracks)};if items.is_empty(){bail!("专辑曲目未取全")}
        }bail!("专辑分页上限")
    }
    pub fn song(&self,mid:&str)->Result<Value>{
        if mid.is_empty()||!mid.bytes().all(|b|b.is_ascii_alphanumeric()){bail!("这首歌没有可查询的 QQ 歌曲编号")}
        let d=self.rpc("music.pf_song_detail_svr","get_song_detail_yqq",json!({"song_mid":mid}),true)?;
        if d["track_info"]["mid"]!=mid{bail!("歌曲详情未找到")};Ok(d["track_info"].clone())
    }
    pub fn search(&self,query:&str)->Result<Vec<Value>>{let d=self.rpc("music.search.SearchCgiService","DoSearchForQQMusicDesktop",json!({"search_type":0,"query":query,"page_num":1,"num_per_page":20}),false)?;Ok(d["body"]["song"]["list"].as_array().context("搜索结果缺失")?.clone())}
    pub fn audio_url(&self,mid:&str,kind:u64)->Result<Url>{
        if !self.authenticated(){bail!("请连接 QQ 账户后播放")}
        if mid.is_empty(){bail!("此条目没有在线歌曲编号，可在 QQ 客户端或通过本地文件播放")}
        let d=self.rpc("vkey.GetVkeyServer","CgiGetVkey",json!({"guid":(millis()%9000000000+1000000000).to_string(),"songmid":[mid],"songtype":[kind],"uin":self.uin(),"loginflag":1,"platform":"20"}),true)?;
        let path=d["midurlinfo"][0]["purl"].as_str().filter(|s|!s.is_empty()).context("QQ 未授予这首歌的播放权限，请检查会员、地区或客户端可播状态")?;
        let base=d["sip"].as_array().into_iter().flatten().filter_map(Value::as_str).find_map(|s|Url::parse(&s.replacen("http://","https://",1)).ok().filter(audio_host)).context("QQ 未提供受支持的音频服务器")?;
        let url=base.join(path).map_err(|_|anyhow::anyhow!("音频地址无效"))?;if !audio_host(&url){bail!("音频服务器不受支持")};Ok(url)
    }
}
pub fn audio_host(url:&Url)->bool {url.scheme()=="https"&&url.username().is_empty()&&url.password().is_none()&&url.host_str().is_some_and(|h|h=="qq.com"||h.ends_with(".qq.com"))}

#[cfg(test)]mod tests{use super::*;#[test]fn trusted_redirect_boundaries(){assert!(allowed(&Url::parse("https://ssl.ptlogin2.graph.qq.com/check_sig").unwrap()));assert!(!allowed(&Url::parse("https://y.qq.com.evil.invalid/").unwrap()));assert!(!allowed(&Url::parse("http://y.qq.com/").unwrap()));}#[test]fn import_keeps_auth_native(){let w=WebClient::new(Some(&json!({"cookies":[{"domain":".qq.com","name":"uin","value":"1"},{"domain":".qq.com","name":"qm_keyst","value":"synthetic"}]}))).unwrap();assert!(w.authenticated());assert_eq!(w.uin(),"1");}}

#[cfg(test)] mod live_auth_test {
    use super::*;
    #[test] #[ignore = "requires explicit in-memory QQ graph authorization"]
    fn rust_oauth_from_existing_graph_authorization() {
        let original:Value=serde_json::from_str(&std::env::var("RHINE_QQ_SESSION").unwrap()).unwrap();
        let graph=original["cookies"].as_array().unwrap().iter().filter(|c|c["domain"].as_str().unwrap_or("").trim_start_matches('.')=="graph.qq.com").cloned().collect::<Vec<_>>();
        let mut web=WebClient::new(Some(&json!({"cookies":graph}))).unwrap();
        assert!(!web.authenticated());
        let result=web.finish_oauth().unwrap();assert_eq!(result["connected"],true);assert!(web.authenticated());
        assert!(web.directory().unwrap()["mydiss"]["list"].is_array());
        println!("Rust OAuth established a fresh QQ Music session from graph authorization and read the personal directory.");
    }
}
