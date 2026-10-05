/** Two stable resolution tiers. A settling delay prevents allocation churn on
 * every input or spring oscillation. The user's saved quality is never changed. */
export class MotionResolution {
  enabled = false;
  scale = 1;
  private lastMotion = -Infinity;
  private pose?: number[];
  reset() { this.scale=1;this.lastMotion=-Infinity;this.pose=undefined; }
  update(seconds:number, pose:readonly number[], active:boolean, pixelCount:number, idle=false) {
    const moved = this.pose !== undefined && pose.some((v,i)=>Math.abs(v-this.pose![i])>.002);
    this.pose=[...pose];
    if (moved) this.lastMotion=seconds;
    // The archive keeps its ambient wave after interaction settles. Its composed
    // camera/card pose still moves, but must not keep re-arming the lower tier.
    this.scale=this.enabled && active && !idle && pixelCount>1_400_000 && seconds-this.lastMotion<.85 ? 2/3 : 1;
    return this.scale;
  }
}
