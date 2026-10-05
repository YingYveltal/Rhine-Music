//! QQ's AAC/MP4 files are decoded by macOS AudioToolbox. Audio remains in the
//! Rust transport and mixer; decoding uses a fixed-size PCM buffer, not a subprocess.
use anyhow::{bail, Result};
use rodio::{Source, source::SeekError};
use std::{ffi::c_void, os::unix::ffi::OsStrExt, path::Path, time::Duration};

#[repr(C)] #[derive(Default)]
struct Format { rate:f64, format:u32, flags:u32, bytes_packet:u32, frames_packet:u32, bytes_frame:u32, channels:u32, bits:u32, reserved:u32 }
#[repr(C)] struct Buffer { channels:u32, size:u32, data:*mut c_void }
#[repr(C)] struct BufferList { count:u32, buffer:Buffer }
#[link(name="AudioToolbox",kind="framework")]
extern "C" {
    fn ExtAudioFileOpenURL(url:*const c_void,file:*mut *mut c_void)->i32;
    fn ExtAudioFileDispose(file:*mut c_void)->i32;
    fn ExtAudioFileGetProperty(file:*mut c_void,id:u32,size:*mut u32,data:*mut c_void)->i32;
    fn ExtAudioFileSetProperty(file:*mut c_void,id:u32,size:u32,data:*const c_void)->i32;
    fn ExtAudioFileRead(file:*mut c_void,frames:*mut u32,data:*mut BufferList)->i32;
    fn ExtAudioFileSeek(file:*mut c_void,frame:i64)->i32;
}
#[link(name="CoreFoundation",kind="framework")]
extern "C" {
    fn CFURLCreateFromFileSystemRepresentation(allocator:*const c_void,bytes:*const u8,len:isize,directory:u8)->*const c_void;
    fn CFRelease(value:*const c_void);
}
pub struct QqAudio { file:*mut c_void, rate:u32, channels:u16, duration:Duration, pcm:Vec<i16>, offset:usize, length:usize }
// The handle is exclusively owned and moved to rodio's audio thread. It is never shared.
unsafe impl Send for QqAudio {}
impl Drop for QqAudio { fn drop(&mut self){unsafe{ExtAudioFileDispose(self.file);}} }
fn check(status:i32)->Result<()> {if status!=0{bail!("macOS 音频解码失败（{status}）")}Ok(())}
impl QqAudio {
    pub fn open(path:&Path)->Result<Self>{
        let bytes=path.as_os_str().as_bytes();
        unsafe {
            let url=CFURLCreateFromFileSystemRepresentation(std::ptr::null(),bytes.as_ptr(),bytes.len() as isize,0);
            if url.is_null(){bail!("音频缓存路径无效")}
            let mut file=std::ptr::null_mut();let status=ExtAudioFileOpenURL(url,&mut file);CFRelease(url);check(status)?;
            let mut source=Self{file,rate:0,channels:0,duration:Duration::ZERO,pcm:vec![],offset:0,length:0};
            let mut format=Format::default();let mut size=std::mem::size_of::<Format>() as u32;
            check(ExtAudioFileGetProperty(file,u32::from_be_bytes(*b"ffmt"),&mut size,&mut format as *mut _ as *mut c_void))?;
            if !(8000. ..=192000.).contains(&format.rate)||format.channels==0||format.channels>8 {bail!("音频参数不受支持")}
            let mut frames=0i64;size=8;
            check(ExtAudioFileGetProperty(file,u32::from_be_bytes(*b"#frm"),&mut size,&mut frames as *mut _ as *mut c_void))?;
            source.rate=format.rate as u32;source.channels=format.channels as u16;
            source.duration=Duration::from_secs_f64(frames.max(0) as f64/format.rate);
            let pcm=Format{rate:format.rate,format:u32::from_be_bytes(*b"lpcm"),flags:12,bytes_packet:2*format.channels,frames_packet:1,bytes_frame:2*format.channels,channels:format.channels,bits:16,reserved:0};
            check(ExtAudioFileSetProperty(file,u32::from_be_bytes(*b"cfmt"),std::mem::size_of::<Format>() as u32,&pcm as *const _ as *const c_void))?;
            source.pcm.resize(4096*format.channels as usize,0);source.refill()?;Ok(source)
        }
    }
    fn refill(&mut self)->Result<()> {
        let mut frames=4096u32;
        let mut buffers=BufferList{count:1,buffer:Buffer{channels:self.channels as u32,size:(self.pcm.len()*2) as u32,data:self.pcm.as_mut_ptr() as *mut c_void}};
        check(unsafe{ExtAudioFileRead(self.file,&mut frames,&mut buffers)})?;
        self.offset=0;self.length=frames as usize*self.channels as usize;Ok(())
    }
}
impl Iterator for QqAudio {type Item=i16;fn next(&mut self)->Option<i16>{
    if self.offset>=self.length && (self.refill().is_err()||self.length==0){return None}
    let sample=self.pcm[self.offset];self.offset+=1;Some(sample)
}}
impl Source for QqAudio {
    fn current_frame_len(&self)->Option<usize>{None}
    fn channels(&self)->u16{self.channels}
    fn sample_rate(&self)->u32{self.rate}
    fn total_duration(&self)->Option<Duration>{Some(self.duration)}
    fn try_seek(&mut self,time:Duration)->std::result::Result<(),SeekError>{
        let frame=(time.min(self.duration).as_secs_f64()*self.rate as f64) as i64;
        let result=unsafe{ExtAudioFileSeek(self.file,frame)};
        if result!=0{return Err(SeekError::Other(Box::new(std::io::Error::other(format!("AudioToolbox seek {result}")))))}
        self.length=0;self.offset=0;Ok(())
    }
}
