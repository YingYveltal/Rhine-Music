// Three.js BokehShader / ACES coefficients retain their MIT attribution; see NOTICE.md.
#include <metal_stdlib>
#include <metal_raytracing>
using namespace metal;
using namespace raytracing;
struct Post { uint width, height, mode, pad; float focus, aspect, aperture, maxblur; float nearClip, farClip, exposure, pad2; };
constant float2 taps[41] = {
 float2(0,0),float2(0,.4),float2(.15,.37),float2(.29,.29),float2(-.37,.15),float2(.4,0),float2(.37,-.15),float2(.29,-.29),float2(-.15,-.37),
 float2(0,-.4),float2(-.15,.37),float2(-.29,.29),float2(.37,.15),float2(-.4,0),float2(-.37,-.15),float2(-.29,-.29),float2(.15,-.37),
 float2(.15,.37)*.9,float2(-.37,.15)*.9,float2(.37,-.15)*.9,float2(-.15,-.37)*.9,float2(-.15,.37)*.9,float2(.37,.15)*.9,float2(-.37,-.15)*.9,float2(.15,-.37)*.9,
 float2(.29,.29)*.7,float2(.4,0)*.7,float2(.29,-.29)*.7,float2(0,-.4)*.7,float2(-.29,.29)*.7,float2(-.4,0)*.7,float2(-.29,-.29)*.7,float2(0,.4)*.7,
 float2(.29,.29)*.4,float2(.4,0)*.4,float2(.29,-.29)*.4,float2(0,-.4)*.4,float2(-.29,.29)*.4,float2(-.4,0)*.4,float2(-.29,-.29)*.4,float2(0,.4)*.4
};
float3 displayColor(float3 color, float exposure) {
 const float3x3 input(float3(.59719,.076,.0284),float3(.35458,.90834,.13383),float3(.04823,.01566,.83777));
 const float3x3 output(float3(1.60475,-.10208,-.00327),float3(-.53108,1.10813,-.07276),float3(-.07367,-.00605,1.07602));
 color = input * (color * (exposure / .6));
 color = (color*(color+.0245786)-.000090537)/(color*(.983729*color+.432951)+.238081);
 color = clamp(output * color,0.,1.);
 return select(1.055*pow(color,float3(.41666))-.055,12.92*color,color<=.0031308);
}
float4 blur(texture2d<float,access::sample> color,texture2d<float,access::read> depth,constant Post &p,uint2 pos) {
 constexpr sampler bilinear(coord::normalized, address::clamp_to_edge, filter::linear);
 float2 size(p.width,p.height), uv=(float2(pos)+.5)/size;
 float d=dot(depth.read(pos),float4(255./256.,255./65536.,255./16777216.,1./16777216.));
 float viewZ=p.nearClip*p.farClip/((p.farClip-p.nearClip)*d-p.farClip);
 float b=clamp((p.focus+viewZ)*p.aperture,-p.maxblur,p.maxblur);
 float2 spread=b*float2(1.,p.aspect),radius=abs(spread)*size;
 float4 c=0;
 if(p.mode!=0 && max(radius.x,radius.y)*.4<=.999) {
  // Coefficients generated from the same 41 taps, not a different blur profile.
  float ax=0,ay=0,axy=0;
  for(uint i=0;i<41;i++){ax+=abs(taps[i].x)/41.;ay+=abs(taps[i].y)/41.;axy+=abs(taps[i].x*taps[i].y)/41.;}
  float w=ax*ay/axy;float2 o=radius*float2(axy/ay,axy/ax)/size;
  c=(color.sample(bilinear,uv+o)+color.sample(bilinear,uv-o)+color.sample(bilinear,uv+float2(o.x,-o.y))+color.sample(bilinear,uv+float2(-o.x,o.y)))*(w*.25)+color.sample(bilinear,uv)*(1.-w);
 } else {
  for(uint i=0;i<41;i++)c+=color.sample(bilinear,uv+taps[i]*spread);
  c/=41.;
 }
 c.a=1.;return c;
}
kernel void bokeh(texture2d<float,access::sample> color[[texture(0)]],texture2d<float,access::read> depth[[texture(1)]],texture2d<float,access::write> dst[[texture(2)]],constant Post &p[[buffer(0)]],uint2 pos[[thread_position_in_grid]]){
 if(pos.x>=p.width||pos.y>=p.height)return;
 dst.write(blur(color,depth,p,pos),pos);
}
kernel void outputColor(texture2d<float,access::read> src[[texture(0)]],texture2d<float,access::write> dst[[texture(2)]],constant Post &p[[buffer(0)]],uint2 pos[[thread_position_in_grid]]){
 if(pos.x>=p.width||pos.y>=p.height)return;
 dst.write(float4(displayColor(src.read(pos).rgb,p.exposure),1),pos);
}
kernel void fused(texture2d<float,access::sample> color[[texture(0)]],texture2d<float,access::read> depth[[texture(1)]],texture2d<float,access::write> dst[[texture(2)]],constant Post &p[[buffer(0)]],uint2 pos[[thread_position_in_grid]]){
 if(pos.x>=p.width||pos.y>=p.height)return;
 // Preserve the original intermediate rgba16f quantization before tone mapping.
 float3 c=float3(half3(blur(color,depth,p,pos).rgb));
 dst.write(float4(displayColor(c,p.exposure),1),pos);
}
struct Camera { float4x4 world, inverseProjection, viewProjection; uint width,height,secondary,pad; };
struct Model { float4x4 matrix; uint id, geometry, pad0,pad1; };
struct Varying { float4 position[[position]]; uint id[[flat]]; float3 world; };
vertex Varying primaryVertex(device packed_float3 *positions[[buffer(0)]],device Model *models[[buffer(1)]],constant Camera &camera[[buffer(2)]],uint vid[[vertex_id]],uint iid[[instance_id]]) {
 Varying o;float4 world=models[iid].matrix*float4(float3(positions[vid]),1);
 o.position=camera.viewProjection*world; o.position.z=(o.position.z+o.position.w)*.5;
 // Texture row zero corresponds to WebGL bottom, shared by the ray generator.
 o.position.y=-o.position.y;o.id=models[iid].id;o.world=world.xyz;return o;
}
fragment float4 primaryFragment(Varying in[[stage_in]]) {return float4(float(in.id+1),in.position.z,0,1);}
float3 unproject(float2 ndc,float z,constant Camera &c){float4 v=c.inverseProjection*float4(ndc,z,1);return (c.world*float4(v.xyz/v.w,1)).xyz;}
kernel void primaryRays(instance_acceleration_structure acceleration[[buffer(0)]],constant Camera &c[[buffer(1)]],texture2d<float,access::write> dst[[texture(0)]],uint2 pos[[thread_position_in_grid]]) {
 if(pos.x>=c.width||pos.y>=c.height)return;
 float2 ndc=(float2(pos)+.5)/float2(c.width,c.height)*2.-1.;
 float3 near=unproject(ndc,-1,c),far=unproject(ndc,1,c);
 ray r; r.origin=near;r.direction=normalize(far-near);r.min_distance=0;r.max_distance=distance(far,near);
 intersector<triangle_data,instancing> test;test.assume_geometry_type(geometry_type::triangle);test.force_opacity(forced_opacity::opaque);
 auto hit=test.intersect(r,acceleration,0xff);
 if(hit.type==intersection_type::none){dst.write(float4(0,1,0,1),pos);return;}
 float3 world=r.origin+r.direction*hit.distance;float4 clip=c.viewProjection*float4(world,1);
 float shadow=0;
 if(c.secondary>0){
  intersector<triangle_data,instancing> shadowTest;shadowTest.assume_geometry_type(geometry_type::triangle);shadowTest.force_opacity(forced_opacity::opaque);shadowTest.accept_any_intersection(true);
  for(uint j=0;j<c.secondary;j++){
   // Fixed, independent diagnostic rays. These are NOT original soft shadows or AO.
   float a=float(j)*2.399963;float3 light=normalize(float3(-.5+cos(a)*.08,1.,.6+sin(a)*.08));
   ray s;s.origin=world+light*.002;s.direction=light;s.min_distance=0;s.max_distance=100.;
   shadow+=shadowTest.intersect(s,acceleration,0xff).type!=intersection_type::none?1.:0.;
  }
  shadow/=float(c.secondary);
 }
 dst.write(float4(float(hit.instance_id+1),clip.z/clip.w*.5+.5,shadow,1),pos);
}
