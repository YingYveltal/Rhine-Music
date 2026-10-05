//! QQMusicMac 11.10.0 (73282) cache schema, opened read-only in a snapshot.
//! K_SONG_RESERVE1 is the song MID, checked against this account's cloud data.
//! Local type codes are intentionally not reused as web song types.
use crate::{Error, Track};
use rusqlite::{params, Connection, OpenFlags};
use serde::Serialize;
use std::{path::{Path, PathBuf}, time::Duration};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPlaylist {
    pub id: String,
    pub name: String,
    pub liked: bool,
    pub advertised_count: usize,
    pub cached_count_matches: bool,
    pub tracks: Vec<Track>,
    pub order: &'static str,
}

pub fn default_path() -> Result<PathBuf, Error> {
    Ok(PathBuf::from(std::env::var_os("HOME").ok_or(Error::LocalDatabase)?)
        .join("Library/Containers/com.tencent.QQMusicMac/Data/Library/Application Support/QQMusicMac/qqmusic.sqlite"))
}

pub fn read_library(path: &Path, account: Option<i64>) -> Result<Vec<LocalPlaylist>, Error> {
    let conn=Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(|_| Error::LocalDatabase)?;
    conn.busy_timeout(Duration::from_secs(3)).map_err(|_| Error::LocalDatabase)?;
    conn.execute_batch("PRAGMA query_only=ON; BEGIN DEFERRED;").map_err(|_| Error::LocalDatabase)?;
    read_snapshot(&conn, account)
}

fn read_snapshot(conn: &Connection, account: Option<i64>) -> Result<Vec<LocalPlaylist>, Error> {
    let accounts: i64 = conn.query_row("SELECT count(DISTINCT uin) FROM NEWFOLDERS WHERE foldertype=1", [], |r| r.get(0))
        .map_err(|_| Error::LocalDatabase)?;
    if accounts > 1 && account.is_none() { return Err(Error::AccountRequired); }
    let mut folders=conn.prepare("SELECT seq,folderTid,folderid,folderName,foldercount FROM NEWFOLDERS
        WHERE foldertype=1 AND (?1 IS NULL OR uin=?1) ORDER BY K_USER_FOLDER_ORDER,seq")
        .map_err(|_| Error::LocalDatabase)?;
    let rows=folders.query_map(params![account], |r| Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,
        r.get::<_,i64>(2)?,r.get::<_,String>(3)?,r.get::<_,usize>(4)?))).map_err(|_| Error::LocalDatabase)?;
    let mut songs=conn.prepare("SELECT fs.id,s.K_SONG_RESERVE1,s.name,s.singer,s.album
        FROM NEWFOLDERSONGS fs LEFT JOIN SONGS s ON s.id=fs.id AND s.type=fs.type
        WHERE fs.seq=?1 ORDER BY fs.rowid").map_err(|_| Error::LocalDatabase)?;
    let mut output=Vec::new();
    for row in rows {
        let (seq,id,folder_id,name,total)=row.map_err(|_| Error::LocalDatabase)?;
        let tracks=songs.query_map(params![seq], |r| {
            let mid=r.get::<_,Option<String>>(1)?.filter(|v| !v.is_empty());
            let detail_url=mid.as_ref().filter(|s| s.bytes().all(|b|b.is_ascii_alphanumeric()))
                .map(|s|format!("https://i2.y.qq.com/a/song/{s}"));
            Ok(Track { id:Some(r.get::<_,i64>(0)?.to_string()), mid,
                title:r.get::<_,Option<String>>(2)?.unwrap_or_default(),
                artists:r.get::<_,Option<String>>(3)?.filter(|s|!s.is_empty()).into_iter().collect(),
                album:r.get::<_,Option<String>>(4)?.filter(|s|!s.is_empty()), duration:None,
                detail_url, source:"qq-local-cache".into() })
        }).map_err(|_|Error::LocalDatabase)?.collect::<Result<Vec<_>,_>>().map_err(|_|Error::LocalDatabase)?;
        output.push(LocalPlaylist {id:id.to_string(),name,liked:folder_id==201,
            advertised_count:total,cached_count_matches:tracks.len()==total,tracks,order:"local-storage-unverified"});
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readonly_join_preserves_missing_metadata_and_duplicates() {
        let dir=tempfile::tempdir().unwrap();
        let path=dir.path().join("synthetic.sqlite");
        let db=Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE NEWFOLDERS(seq,uin,folderTid,folderid,foldertype,folderName,foldercount,K_USER_FOLDER_ORDER);
          CREATE TABLE NEWFOLDERSONGS(seq,id,type);
          CREATE TABLE SONGS(id,type,K_SONG_RESERVE1,name,singer,album);
          INSERT INTO NEWFOLDERS VALUES(1,1,123,201,1,'fixture',3,0);
          INSERT INTO SONGS VALUES(10,13,'mid','title','artist','album');
          INSERT INTO NEWFOLDERSONGS VALUES(1,10,13),(1,10,13),(1,11,21);").unwrap();
        drop(db);
        let before=std::fs::read(&path).unwrap();
        let items=read_library(&path,None).unwrap();
        assert_eq!(items[0].tracks.len(),3);
        assert_eq!(items[0].tracks[2].id.as_deref(),Some("11"));
        assert!(items[0].tracks[2].mid.is_none());
        assert_eq!(before,std::fs::read(&path).unwrap());
        let missing=dir.path().join("missing.sqlite");
        assert!(read_library(&missing,None).is_err());
        assert!(!missing.exists());
    }
    #[test]
    fn ambiguous_accounts_are_not_merged() {
        let db=Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE NEWFOLDERS(uin,foldertype);
          INSERT INTO NEWFOLDERS VALUES(1,1),(2,1);").unwrap();
        assert!(matches!(read_snapshot(&db,None),Err(Error::AccountRequired)));
    }
}
