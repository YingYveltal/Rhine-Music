#import <Cocoa/Cocoa.h>
#import "MusicAPI.h"

static NSString *FourCC(NSInteger code) {
    char s[5]={(code>>24)&255,(code>>16)&255,(code>>8)&255,code&255,0};
    return [[NSString alloc] initWithBytes:s length:4 encoding:NSASCIIStringEncoding] ?: @"unknown";
}
static NSString *Cloud(NSInteger code) {
    if(code=='kSub') return @"Apple Music 订阅";
    if(code=='kPur') return @"已购买";
    if(code=='kMat') return @"匹配";
    if(code=='kUpl') return @"已上传";
    return FourCC(code);
}
@interface Probe : NSObject <NSApplicationDelegate,NSTableViewDataSource,NSTableViewDelegate,SBApplicationDelegate>
@property NSWindow *window;
@property NSTextField *status,*now,*seek;
@property NSPopUpButton *lists;
@property NSTableView *table;
@property NSMutableArray<NSButton *> *controls;
@property NSArray *playlists,*tracks,*rows;
@property RMMusic *music;
@property NSError *lastError;
@property dispatch_queue_t queue;
@property BOOL busy,connected;
@property NSURL *logURL;
@end
@implementation Probe
- (void)log:(NSString *)event data:(id)data {
    // Construct timestamp explicitly; all metadata stays in the local build output.
    NSDictionary *row=@{@"time":[NSISO8601DateFormatter.new stringFromDate:NSDate.date],@"event":event,@"data":data ?: @{}};
    NSData *bytes=[NSJSONSerialization dataWithJSONObject:row options:0 error:nil];
    if(!bytes)return;
    NSMutableData *line=[bytes mutableCopy];[line appendData:[@"\n" dataUsingEncoding:NSUTF8StringEncoding]];
    if(![NSFileManager.defaultManager fileExistsAtPath:self.logURL.path]) [NSData.data writeToURL:self.logURL atomically:YES];
    NSFileHandle *file=[NSFileHandle fileHandleForWritingToURL:self.logURL error:nil];
    @try{[file seekToEndOfFile];[file writeData:line];[file closeFile];}@catch(NSException *e){}
}
- (id)eventDidFail:(const AppleEvent *)event withError:(NSError *)error {self.lastError=error;return nil;}
- (NSTextField *)label:(NSString *)text frame:(NSRect)frame {
    NSTextField *label=[NSTextField wrappingLabelWithString:text];label.frame=frame;[self.window.contentView addSubview:label];return label;
}
- (NSButton *)button:(NSString *)title action:(SEL)action frame:(NSRect)frame control:(BOOL)control {
    NSButton *button=[NSButton buttonWithTitle:title target:self action:action];button.frame=frame;[self.window.contentView addSubview:button];
    if(control){button.enabled=NO;[self.controls addObject:button];}return button;
}
- (void)applicationDidFinishLaunching:(NSNotification *)notification {
    self.queue=dispatch_queue_create("com.rhine.music.appleprobe.events",DISPATCH_QUEUE_SERIAL);
    self.controls=NSMutableArray.new;self.playlists=@[];self.tracks=@[];self.rows=@[];
    self.music=(RMMusic *)[SBApplication applicationWithBundleIdentifier:@"com.apple.Music"];
    self.music.delegate=self;self.music.timeout=60*30;
    NSString *dir=[NSBundle.mainBundle objectForInfoDictionaryKey:@"RhineProbeEvidenceDirectory"];
    [NSFileManager.defaultManager createDirectoryAtPath:dir withIntermediateDirectories:YES attributes:nil error:nil];
    self.logURL=[NSURL fileURLWithPath:[dir stringByAppendingPathComponent:@"events.jsonl"]];
    self.window=[[NSWindow alloc] initWithContentRect:NSMakeRect(0,0,1040,740) styleMask:NSWindowStyleMaskTitled|NSWindowStyleMaskClosable|NSWindowStyleMaskMiniaturizable backing:NSBackingStoreBuffered defer:NO];
    self.window.title=@"Rhine Apple Music Probe";[self.window center];
    [self label:@"Apple Music 本机连接验证" frame:NSMakeRect(20,690,980,28)].font=[NSFont boldSystemFontOfSize:22];
    [self label:@"读取 Music.app 资料库并遥控播放；两边共享歌曲与播放队列。不会修改歌单，不需要开发者 API 令牌。" frame:NSMakeRect(20,650,1000,34)];
    [self button:@"连接并读取歌单" action:@selector(connect:) frame:NSMakeRect(20,605,190,34) control:NO];
    self.status=[self label:[NSString stringWithFormat:@"Music.app %@ · 尚未连接",self.music.running?@"正在运行":@"未运行"] frame:NSMakeRect(225,598,790,42)];
    self.lists=[[NSPopUpButton alloc] initWithFrame:NSMakeRect(20,551,630,34) pullsDown:NO];self.lists.target=self;self.lists.action=@selector(loadTracks:);self.lists.enabled=NO;[self.window.contentView addSubview:self.lists];
    [self button:@"读取所选歌单" action:@selector(loadTracks:) frame:NSMakeRect(665,551,170,34) control:YES];
    self.table=[[NSTableView alloc] initWithFrame:NSZeroRect];
    NSArray *names=@[@"顺序",@"曲目",@"歌手",@"来源 / 格式",@"时长"];
    NSArray *widths=@[@50,@300,@180,@310,@80];
    for(NSUInteger i=0;i<names.count;i++){NSTableColumn *column=[[NSTableColumn alloc] initWithIdentifier:[NSString stringWithFormat:@"%lu",i]];column.title=names[i];column.width=[widths[i] doubleValue];[self.table addTableColumn:column];}
    self.table.dataSource=self;self.table.delegate=self;self.table.rowHeight=26;self.table.allowsMultipleSelection=NO;
    NSScrollView *scroll=[[NSScrollView alloc] initWithFrame:NSMakeRect(20,205,1000,330)];scroll.documentView=self.table;scroll.hasVerticalScroller=YES;[self.window.contentView addSubview:scroll];
    NSArray *titles=@[@"播放所选曲目",@"暂停",@"继续播放",@"上一首",@"下一首",@"停止",@"刷新状态"];
    NSArray *actions=@[NSStringFromSelector(@selector(playSelected:)),NSStringFromSelector(@selector(pause:)),NSStringFromSelector(@selector(resume:)),NSStringFromSelector(@selector(previous:)),NSStringFromSelector(@selector(next:)),NSStringFromSelector(@selector(stop:)),NSStringFromSelector(@selector(refresh:))];
    for(NSUInteger i=0;i<titles.count;i++)[self button:titles[i] action:NSSelectorFromString(actions[i]) frame:NSMakeRect(20+i*140,155,135,32) control:YES];
    [self label:@"定位秒数" frame:NSMakeRect(20,112,80,25)];
    self.seek=[[NSTextField alloc] initWithFrame:NSMakeRect(100,110,100,28)];self.seek.stringValue=@"60";self.seek.accessibilityLabel=@"定位秒数";[self.window.contentView addSubview:self.seek];
    [self button:@"跳转" action:@selector(seekTo:) frame:NSMakeRect(210,109,85,30) control:YES];
    self.now=[self label:@"尚未读取播放状态。命令成功不代表已经出声，需同时观察实际进度并确认听感。" frame:NSMakeRect(20,20,1000,75)];
    NSMenu *menu=NSMenu.new;NSMenuItem *app=NSMenuItem.new;[menu addItem:app];NSMenu *submenu=NSMenu.new;[submenu addItemWithTitle:@"退出探针" action:@selector(terminate:) keyEquivalent:@"q"];app.submenu=submenu;NSApp.mainMenu=menu;
    [self.window makeKeyAndOrderFront:nil];[NSApp activateIgnoringOtherApps:YES];
    [self log:@"launch" data:@{@"musicRunning":@(self.music.running),@"bundleID":NSBundle.mainBundle.bundleIdentifier}];
    [NSTimer scheduledTimerWithTimeInterval:2 target:self selector:@selector(tick:) userInfo:nil repeats:YES];
}
- (void)run:(NSString *)name work:(void (^)(void))work completion:(void (^)(BOOL))completion {
    if(self.busy)return;self.busy=YES;self.status.stringValue=[name stringByAppendingString:@"…"];
    dispatch_async(self.queue,^{
        self.lastError=nil;
        @try{work();}@catch(NSException *e){self.lastError=[NSError errorWithDomain:@"RhineProbe" code:-1 userInfo:@{NSLocalizedDescriptionKey:e.reason?:@"接口异常"}];}
        NSError *error=self.lastError;
        [self log:name data:error?@{@"ok":@NO,@"code":@(error.code),@"message":error.localizedDescription}:@{@"ok":@YES}];
        dispatch_async(dispatch_get_main_queue(),^{
            self.busy=NO;
            self.status.stringValue=error?[NSString stringWithFormat:@"%@失败（%ld）：%@",name,error.code,error.localizedDescription]:[name stringByAppendingString:@"完成"];
            if(completion)completion(!error);
        });
    });
}
- (void)connect:(id)sender {
    __block NSArray *items;
    [self run:@"连接" work:^{
        NSString *version=self.music.version;if(self.lastError)return;
        items=[[self.music playlists] get];
        if(!self.lastError)[self log:@"library" data:@{@"version":version?:@"",@"playlistCount":@(items.count)}];
    } completion:^(BOOL ok){
        self.connected=ok;
        for(NSButton *button in self.controls)button.enabled=ok;
        self.lists.enabled=ok;
        if(!ok)return;
        self.playlists=items?:@[];[self.lists removeAllItems];
        // Names are fetched on the serial event queue, not during AppKit layout.
        [self run:@"歌单名称" work:^{
            NSMutableArray *names=NSMutableArray.new;
            for(RMPlaylist *p in self.playlists){[names addObject:p.name?:@"未命名"];if(self.lastError)break;}
            dispatch_async(dispatch_get_main_queue(),^{[self.lists addItemsWithTitles:names];});
        } completion:^(BOOL success){if(success)self.status.stringValue=[NSString stringWithFormat:@"已连接 · %lu 个可见歌单（Music.app 资料库，不是全站目录）",self.playlists.count];}];
    }];
}
- (void)loadTracks:(id)sender {
    NSInteger index=self.lists.indexOfSelectedItem;if(index<0||(NSUInteger)index>=self.playlists.count||self.busy)return;
    RMPlaylist *list=self.playlists[index];__block NSArray *tracks,*rows;__block NSUInteger total=0;
    [self run:@"读取曲目" work:^{
        SBElementArray *elements=list.tracks;total=elements.count;if(self.lastError)return;
        NSMutableArray *objects=NSMutableArray.new,*data=NSMutableArray.new;
        for(NSUInteger i=0;i<MIN(total,250);i++){
            RMTrack *t=elements[i];NSString *name=t.name?:@"",*artist=t.artist?:@"",*kind=t.kind?:@"";NSInteger cloud=t.cloudStatus;double duration=t.duration;
            if(self.lastError)break;
            [objects addObject:t];[data addObject:@{@"order":@(i+1),@"name":name,@"artist":artist,@"kind":kind,@"cloud":FourCC(cloud),@"duration":@(duration),@"source":[NSString stringWithFormat:@"%@ / %@",Cloud(cloud),kind]}];
        }
        tracks=objects;rows=data;[self log:@"playlistTracks" data:@{@"total":@(total),@"rows":rows}];
    } completion:^(BOOL ok){if(!ok)return;self.tracks=tracks;self.rows=rows;[self.table reloadData];if(rows.count)[self.table selectRowIndexes:[NSIndexSet indexSetWithIndex:0] byExtendingSelection:NO];self.status.stringValue=[NSString stringWithFormat:@"读取 %lu / %lu 首，保留接口顺序（最多显示前 250 首）",rows.count,total];}];
}
- (NSInteger)numberOfRowsInTableView:(NSTableView *)tableView{return self.rows.count;}
- (id)tableView:(NSTableView *)tableView objectValueForTableColumn:(NSTableColumn *)column row:(NSInteger)row {
    NSDictionary *item=self.rows[row];return item[@[@"order",@"name",@"artist",@"source",@"duration"][column.identifier.integerValue]];
}
- (void)playSelected:(id)sender {NSInteger row=self.table.selectedRow;if(row<0||(NSUInteger)row>=self.tracks.count)return;RMTrack *track=self.tracks[row];[self run:@"播放所选" work:^{[self.music play:track once:NO];} completion:nil];}
- (void)pause:(id)sender {[self run:@"暂停" work:^{[self.music pause];} completion:nil];}
- (void)resume:(id)sender {[self run:@"继续播放" work:^{[self.music play:nil once:NO];} completion:nil];}
- (void)previous:(id)sender {[self run:@"上一首" work:^{[self.music previousTrack];} completion:nil];}
- (void)next:(id)sender {[self run:@"下一首" work:^{[self.music nextTrack];} completion:nil];}
- (void)stop:(id)sender {[self run:@"停止" work:^{[self.music stop];} completion:nil];}
- (void)seekTo:(id)sender {double seconds=self.seek.doubleValue;if(!isfinite(seconds)||seconds<0){self.status.stringValue=@"定位秒数必须为非负数";return;}[self run:@"定位" work:^{self.music.playerPosition=seconds;} completion:nil];}
- (void)refresh:(id)sender {
    if(!self.connected||self.busy)return;
    if(!self.music.running){self.now.stringValue=@"Music.app 未运行；点击连接可以重新启动并读取。";return;}
    __block NSDictionary *state;
    [self run:@"播放状态" work:^{
        NSInteger code=self.music.playerState;double position=self.music.playerPosition;if(self.lastError)return;
        NSString *name=@"",*kind=@"",*cloud=@"";double duration=0;
        if(code!='kPSS'){RMTrack *track=self.music.currentTrack;name=track.name?:@"";duration=track.duration;kind=track.kind?:@"";cloud=Cloud(track.cloudStatus);}
        state=@{@"state":FourCC(code),@"position":@(position),@"duration":@(duration),@"track":name,@"kind":kind,@"cloud":cloud,@"volume":@(self.music.soundVolume),@"mute":@(self.music.mute)};
        if(!self.lastError)[self log:@"player" data:state];
    } completion:^(BOOL ok){if(ok)self.now.stringValue=[NSString stringWithFormat:@"状态 %@ · %.1f / %.1f 秒 · 音量 %@ · 静音 %@\n%@\n%@ / %@",state[@"state"],[state[@"position"] doubleValue],[state[@"duration"] doubleValue],state[@"volume"],state[@"mute"],state[@"track"],state[@"cloud"],state[@"kind"]];}];
}
- (void)tick:(NSTimer *)timer {[self refresh:nil];}
- (BOOL)applicationShouldTerminateAfterLastWindowClosed:(NSApplication *)sender{return YES;}
@end
int main(int argc,const char *argv[]){@autoreleasepool{NSApplication *app=NSApplication.sharedApplication;app.activationPolicy=NSApplicationActivationPolicyRegular;Probe *delegate=Probe.new;app.delegate=delegate;[app run];}return 0;}
