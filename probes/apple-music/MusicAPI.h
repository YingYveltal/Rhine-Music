// Minimal read/control declarations from Music.app's public com.apple.Music.sdef.
// Scripting Bridge generates implementations at runtime; no private framework.
#import <ScriptingBridge/ScriptingBridge.h>
@interface RMTrack : SBObject
@property(readonly) NSString *name;
@property(readonly) NSString *persistentID;
@property(readonly) NSString *artist;
@property(readonly) NSString *kind;
@property(readonly) double duration;
@property(readonly) NSInteger cloudStatus;
- (void)playOnce:(BOOL)once;
@end
@interface RMPlaylist : SBObject
@property(readonly) NSString *name;
@property(readonly) NSString *persistentID;
- (SBElementArray *)tracks;
@end
@interface RMMusic : SBApplication
@property(readonly) NSString *version;
@property(readonly) BOOL fixedIndexing,shuffleEnabled;
@property(readonly) NSInteger songRepeat;
@property(readonly) RMPlaylist *currentPlaylist;
@property(readonly) NSInteger playerState;
@property double playerPosition;
@property(readonly) RMTrack *currentTrack;
@property(readonly) NSInteger soundVolume;
@property(readonly) BOOL mute;
- (SBElementArray *)playlists;
- (void)play:(id)item once:(BOOL)once;
- (void)pause;
- (void)stop;
- (void)nextTrack;
- (void)previousTrack;
@end
