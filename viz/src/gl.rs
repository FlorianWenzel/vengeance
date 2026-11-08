//! WebGL2 drawing: a ray-cast floor grid, instanced box faces, instanced screen-space lines
//! (crisp at any DPR, with hidden-line passes) and additive LED glows.

use glam::Mat4;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{HtmlCanvasElement, WebGl2RenderingContext as GL, WebGlBuffer, WebGlProgram, WebGlShader, WebGlUniformLocation, WebGlVertexArrayObject};

use crate::engine::Engine;
use crate::geo::{FACE_STRIDE, LED_STRIDE, LINE_STRIDE};

const COMMON: &str = r#"
uniform mat4 uVP;
uniform vec3 uEye;
uniform vec2 uViewport;
uniform float uDpr;
uniform float uTime;
uniform float uMotion;
uniform vec2 uFog;
uniform float uWalk;
uniform vec3 uFogColor;
uniform vec3 uMoon;
uniform vec4 uLightPos[48];
uniform vec4 uLightCol[48];
uniform int uLightCount;
bool flag(float f, float bit) { return mod(floor(f / bit + 0.001), 2.0) >= 1.0; }
float lodVis(vec3 anchor, float nearD, float farD) {
  float d = distance(uEye, anchor);
  float o = farD <= 0.0 ? 1.0 : 1.0 - smoothstep(farD * 0.8, farD, d);
  float i = nearD <= 0.0 ? 1.0 : smoothstep(nearD * 0.8, nearD, d);
  return o * i;
}
float blinkMul(float flags) {
  if (!flag(flags, 1.0)) return 1.0;
  if (uMotion < 0.5) return 0.5;
  if (uWalk > 0.5) {
    // Neon / bulb flicker: mostly on, with irregular drop-outs.
    float t = uTime / 1000.0;
    float f = sin(t * 13.0) * sin(t * 7.3 + 1.7) * sin(t * 2.1);
    return f > 0.25 ? 0.15 : 1.0;
  }
  return 0.22 + 0.78 * (0.5 + 0.5 * cos(uTime * 6.2831853 / 2600.0));
}
float fogMul(vec3 p) {
  float d = distance(uEye, p);
  return 1.0 - 0.82 * clamp((d - uFog.x) / max(uFog.y - uFog.x, 0.001), 0.0, 1.0);
}
float cityFog(vec3 p) {
  float d = distance(uEye, p);
  float f = clamp((d - uFog.x) / max(uFog.y - uFog.x, 0.001), 0.0, 1.0);
  return f * (2.0 - f);
}
vec3 pointLights(vec3 p, vec3 n) {
  vec3 acc = vec3(0.0);
  for (int i = 0; i < 48; i++) {
    if (i >= uLightCount) break;
    vec3 L = uLightPos[i].xyz - p;
    float d = length(L);
    float r = uLightPos[i].w;
    float att = 1.0 / (1.0 + d * d / (r * r * 0.12));
    att *= 1.0 - smoothstep(r * 0.7, r * 1.5, d);
    float lam = max(dot(n, L / max(d, 1e-4)), 0.0) * 0.85 + 0.15;
    acc += uLightCol[i].rgb * uLightCol[i].w * att * lam;
  }
  return acc;
}
"#;

const LINE_VS: &str = r#"
layout(location=0) in vec2 aCorner;
layout(location=1) in vec3 aP0;
layout(location=2) in vec3 aP1;
layout(location=3) in vec4 aColor;
layout(location=4) in vec3 aAnchor;
layout(location=5) in vec4 aParams; // near, far, flags, width (css px)
uniform float uHidden;
uniform float uAlpha;
uniform float uLinePass; // 0 all, 1 ink (non-glow) only, 2 glow only
out vec4 vColor;
out float vSide;
out float vHalf;
out vec2 vAlong;
flat out float vDash;
void main() {
  float vis = lodVis(aAnchor, aParams.x, aParams.y);
  float flags = aParams.z;
  bool glow = flag(flags, 512.0);
  vec4 c0 = uVP * vec4(aP0, 1.0);
  vec4 c1 = uVP * vec4(aP1, 1.0);
  float eps = 1e-3;
  if (vis < 0.004 || (uHidden > 0.5 && flag(flags, 16.0)) || (c0.w < eps && c1.w < eps)
      || (uLinePass > 0.5 && uLinePass < 1.5 && glow) || (uLinePass > 1.5 && !glow)) {
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    return;
  }
  if (c0.w < eps) c0 = mix(c0, c1, (eps - c0.w) / (c1.w - c0.w));
  else if (c1.w < eps) c1 = mix(c1, c0, (eps - c1.w) / (c0.w - c1.w));
  vec2 s0 = c0.xy / c0.w * 0.5 * uViewport;
  vec2 s1 = c1.xy / c1.w * 0.5 * uViewport;
  vec2 d = s1 - s0;
  float len = length(d);
  vec2 dir = len > 1e-4 ? d / len : vec2(1.0, 0.0);
  vec2 n = vec2(-dir.y, dir.x);
  float wpx = aParams.w * uDpr;
  float w = max(wpx, 1.0);
  float h = w * 0.5 + 1.0;
  vec4 c = aCorner.x < 0.5 ? c0 : c1;
  float ext = (aCorner.x * 2.0 - 1.0) * w * 0.5;
  vec2 off = n * aCorner.y * h + dir * ext;
  c.xy += off / (0.5 * uViewport) * c.w;
  gl_Position = c;
  float along = aCorner.x * len + ext;
  vAlong = vec2(along * c.w, c.w);
  vSide = aCorner.y * h;
  vHalf = w * 0.5;
  vec3 mid = mix(aP0, aP1, aCorner.x);
  float fog = uWalk > 0.5 ? 1.0 - 0.95 * cityFog(mid) : fogMul(mid);
  float a = aColor.a * vis * blinkMul(flags) * fog * uAlpha;
  if (wpx < 1.0) a *= wpx;
  vColor = vec4(aColor.rgb, a);
  vDash = (flag(flags, 2.0) || uHidden > 0.5) ? 1.0 : 0.0;
}
"#;

const LINE_FS: &str = r#"
uniform float uHidden;
in vec4 vColor;
in float vSide;
in float vHalf;
in vec2 vAlong;
flat in float vDash;
out vec4 outColor;
void main() {
  float cov = clamp(vHalf + 0.5 - abs(vSide), 0.0, 1.0);
  if (vDash > 0.5) {
    float s = vAlong.x / vAlong.y;
    float period = (uHidden > 0.5 ? 5.0 : 9.0) * uDpr;
    if (mod(s, period) > period * 0.55) discard;
  }
  outColor = vec4(vColor.rgb, vColor.a * cov);
}
"#;

const FACE_VS: &str = r#"
layout(location=0) in vec3 aPos;
layout(location=1) in vec3 aNormal;
layout(location=2) in vec3 aMin;
layout(location=3) in vec3 aSize;
layout(location=4) in vec4 aColor;
layout(location=5) in vec3 aAnchor;
layout(location=6) in vec3 aParams; // near, far, flags
uniform float uDepthOnly;
out vec4 vColor;
out vec3 vWorld;
out vec3 vNormal;
flat out float vFlags;
void main() {
  float vis = lodVis(aAnchor, aParams.x, aParams.y);
  float flags = aParams.z;
  bool occ = flag(flags, 4.0);
  if (vis < 0.004 || (uDepthOnly > 0.5 && (!occ || vis < 0.5))) {
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    return;
  }
  vec3 p = aMin + aPos * aSize;
  vWorld = p;
  vNormal = aNormal;
  vFlags = flags;
  gl_Position = uVP * vec4(p, 1.0);
  float b = blinkMul(flags);
  // Emissive surfaces flicker in brightness, everything else in opacity.
  vColor = flag(flags, 64.0) ? vec4(aColor.rgb * b, aColor.a * vis) : vec4(aColor.rgb, aColor.a * vis * b);
}
"#;

const FACE_FS: &str = r#"
in vec4 vColor;
in vec3 vWorld;
in vec3 vNormal;
flat in float vFlags;
out vec4 outColor;
void main() {
  vec3 n = normalize(vNormal);
  if (flag(vFlags, 8.0)) {
    float v = (vWorld.x + vWorld.y) / 0.009;
    float f = abs(fract(v) - 0.5);
    float aa = fwidth(v) * 0.75;
    float stripe = 1.0 - smoothstep(0.12 - aa, 0.12 + aa, f);
    outColor = vec4(1.0, 0.54, 0.24, (0.04 + 0.28 * stripe) * vColor.a * fogMul(vWorld));
    return;
  }
  if (uWalk > 0.5) {
    vec3 col;
    if (flag(vFlags, 64.0)) {
      col = vColor.rgb;
    } else {
      float moon = max(dot(n, uMoon), 0.0);
      vec3 amb = vec3(0.16, 0.18, 0.24) * (0.75 + 0.25 * n.y);
      vec3 pl = pointLights(vWorld, n);
      pl = pl / (1.0 + pl * 0.6);
      col = vColor.rgb * (amb + vec3(0.42, 0.48, 0.62) * moon) + vColor.rgb * pl * 1.6;
      // Faint height gradient: street level picks up the glow of the city.
      col += vec3(0.010, 0.008, 0.006) * (1.0 - smoothstep(0.0, 12.0, vWorld.y));
    }
    float f = cityFog(vWorld);
    col = mix(col, uFogColor, f * (flag(vFlags, 64.0) ? 0.75 : 1.0));
    outColor = vec4(col, vColor.a);
    return;
  }
  float shade = 0.7 + 0.3 * max(dot(n, normalize(vec3(0.25, 1.0, 0.55))), 0.0);
  outColor = vec4(vColor.rgb * shade, vColor.a * fogMul(vWorld));
}
"#;

const LED_VS: &str = r#"
layout(location=0) in vec2 aCorner;
layout(location=1) in vec4 aCenterSize;
layout(location=2) in vec4 aColor;
layout(location=3) in vec3 aAnchor;
layout(location=4) in vec4 aParams; // near, far, flags, min px
layout(location=5) in float aPhase;
uniform float uProjY;
out vec4 vColor;
out vec2 vUv;
out float vPx;
flat out float vKind; // 0 led, 1 alert led, 2 fire, 3 smoke
void main() {
  float vis = lodVis(aAnchor, aParams.x, aParams.y);
  vec3 center = aCenterSize.xyz;
  float size = aCenterSize.w;
  vec3 col = aColor.rgb;
  float k = 1.0;
  float kind = flag(aParams.z, 32.0) ? 1.0 : 0.0;
  bool fire = flag(aParams.z, 128.0);
  bool smoke = flag(aParams.z, 256.0);
  if (fire || smoke) {
    float period = fire ? 1100.0 : 4200.0;
    float t = uMotion > 0.5 ? fract(uTime / period + aPhase) : fract(aPhase * 0.83 + 0.1);
    if (fire) {
      center.y += t * size * 2.4;
      center.x += sin(aPhase * 40.0 + t * 7.0) * size * 0.25;
      size *= 1.0 - 0.6 * t;
      col = mix(vec3(1.0, 0.86, 0.4), vec3(1.0, 0.22, 0.04), t);
      k = (1.0 - t) * smoothstep(0.0, 0.12, t) * 1.4;
      kind = 2.0;
    } else {
      center.y += t * size * 5.5;
      center.x += sin(aPhase * 20.0 + t * 2.5) * size * 0.9;
      size *= 1.0 + 2.4 * t;
      k = sin(3.14159 * t) * 0.7;
      kind = 3.0;
    }
  }
  // Lift the sprite towards the eye so its halo is not clipped by the panel it sits on.
  vec3 toEye = uEye - center;
  vec3 lifted = center + normalize(toEye) * min(size * (kind > 1.5 ? 0.5 : 3.0), length(toEye) * 0.5);
  vec4 c = uVP * vec4(lifted, 1.0);
  if (vis < 0.004 || c.w < 1e-3 || k <= 0.0) { gl_Position = vec4(2.0, 2.0, 2.0, 1.0); return; }
  float pxPerWorld = uViewport.y * 0.5 * uProjY / c.w;
  float sizePx = max(size * pxPerWorld, aParams.w * uDpr);
  float r = kind > 1.5 ? sizePx * 1.7 + 1.0 : sizePx * (kind > 0.5 ? 3.6 : 2.6) + 2.0;
  c.xy += aCorner * r / (0.5 * uViewport) * c.w;
  gl_Position = c;
  vUv = aCorner * r / sizePx;
  vPx = sizePx;
  vKind = kind;
  float pulse = 1.0;
  if (kind == 1.0 && uMotion > 0.5) pulse = 0.8 + 0.2 * sin(uTime * 6.2831853 / 1800.0);
  float fog = uWalk > 0.5 ? 1.0 - 0.9 * cityFog(center) : fogMul(center);
  vColor = vec4(col, vis * blinkMul(aParams.z) * fog * pulse * k);
}
"#;

const LED_FS: &str = r#"
in vec4 vColor;
in vec2 vUv;
in float vPx;
flat in float vKind;
out vec4 outColor;
void main() {
  // Premultiplied output: alpha 0 = additive glow, alpha > 0 = covers what is behind (smoke).
  if (vKind > 2.5) {
    float a = exp(-dot(vUv, vUv) * 2.2) * vColor.a;
    outColor = vec4(vec3(0.05, 0.05, 0.055) * a, a * 0.85);
    return;
  }
  if (vKind > 1.5) {
    float a = exp(-dot(vUv, vUv) * 2.6) * vColor.a;
    outColor = vec4(vColor.rgb * a, 0.0);
    return;
  }
  vec2 q = abs(vUv) - vec2(0.32);
  float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - 0.18;
  float core = 1.0 - smoothstep(-1.0 / vPx, 1.0 / vPx, d);
  // In the city, big lamps are soft glows rather than LED squares.
  if (uWalk > 0.5) core *= 1.0 - smoothstep(8.0, 22.0, vPx);
  float r2 = dot(vUv, vUv);
  float alert = vKind > 0.5 ? 1.0 : 0.0;
  float halo = exp(-r2 * mix(0.9, 0.45, alert)) * mix(0.32, 0.6, alert);
  vec3 col = vColor.rgb * halo + mix(vColor.rgb, vec3(1.0), 0.18) * core * 0.9;
  outColor = vec4(col * vColor.a, 0.0);
}
"#;

const BG_VS: &str = r#"
out vec2 vNdc;
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2)) * 2.0 - 1.0;
  vNdc = p;
  gl_Position = vec4(p, 0.0, 1.0);
}
"#;

const BG_FS: &str = r#"
uniform mat4 uInvVP;
uniform vec4 uScene; // center xz, radius, unused
uniform float uWaterX;
in vec2 vNdc;
out vec4 outColor;
float grid(vec2 p, float s, float w) {
  vec2 q = p / s;
  vec2 fw = fwidth(q);
  vec2 g = abs(fract(q - 0.5) - 0.5) / max(fw, vec2(1e-5));
  float l = min(g.x, g.y);
  float fade = 1.0 - smoothstep(0.12, 0.45, max(fw.x, fw.y));
  return (1.0 - min(l / w, 1.0)) * fade;
}
float hash2(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
float vnoise(vec2 p) {
  vec2 i = floor(p);
  vec2 f = fract(p);
  vec2 u = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash2(i), hash2(i + vec2(1, 0)), u.x), mix(hash2(i + vec2(0, 1)), hash2(i + vec2(1, 1)), u.x), u.y);
}
vec3 city(vec3 dir) {
  vec3 sky = mix(uFogColor * 1.1, vec3(0.006, 0.009, 0.022), smoothstep(-0.02, 0.5, dir.y));
  float md = max(dot(dir, uMoon), 0.0);
  sky += vec3(0.92, 0.94, 1.0) * smoothstep(0.99955, 0.99975, md);
  sky += vec3(0.30, 0.36, 0.5) * pow(md, 90.0) * 0.6 + vec3(0.10, 0.12, 0.18) * pow(md, 8.0) * 0.35;
  if (dir.y > 0.04) {
    vec3 q = floor(dir * 260.0);
    float h = fract(sin(dot(q, vec3(12.9898, 78.233, 45.164))) * 43758.5453);
    sky += vec3(0.55, 0.6, 0.75) * step(0.9982, h) * smoothstep(0.04, 0.3, dir.y) * (1.0 - smoothstep(0.995, 1.0, md));
    // Thin moonlit cloud bands.
    float cl = vnoise(dir.xz / max(dir.y, 0.05) * 1.6 + vec2(uTime * 0.00001 * uMotion, 0.0));
    sky += vec3(0.05, 0.06, 0.085) * smoothstep(0.55, 0.85, cl) * (0.4 + pow(md, 4.0)) * smoothstep(0.04, 0.25, dir.y);
  }
  if (dir.y >= -1e-4) return sky;
  float t = -uEye.y / dir.y;
  vec3 P = uEye + dir * t;
  bool water = P.x > uWaterX;
  float plane = 0.0;
  if (water) {
    plane = -0.9;
    t = (uEye.y - plane) / (-dir.y);
    P = uEye + dir * t;
  }
  float eyeH = uEye.y - plane;
  vec3 col = water ? vec3(0.008, 0.014, 0.024) : vec3(0.026, 0.027, 0.033);
  float n = vnoise(P.xz * 0.3) * 0.6 + vnoise(P.xz * 1.4) * 0.4;
  float wet = water ? 1.0 : smoothstep(0.38, 0.62, n);
  if (!water) col *= mix(1.0, 0.7, wet);
  float ripple = water ? (vnoise(P.xz * vec2(0.6, 2.5) + vec2(0.0, uTime * 0.0004 * uMotion)) - 0.5) : 0.0;
  for (int i = 0; i < 48; i++) {
    if (i >= uLightCount) break;
    vec3 L = uLightPos[i].xyz;
    vec3 lc = uLightCol[i].rgb * uLightCol[i].w;
    float ly = L.y - plane;
    vec2 Q = uEye.xz + (L.xz - uEye.xz) * (eyeH / (eyeH + ly));
    vec2 rad = Q - uEye.xz;
    float rl = length(rad);
    rad = rl > 1e-3 ? rad / rl : vec2(1.0, 0.0);
    vec2 dq = P.xz - Q;
    float along = dot(dq, rad);
    float perp = dot(dq, vec2(-rad.y, rad.x)) + ripple * (0.6 + 0.04 * rl);
    float sw = 0.10 + 0.012 * rl;
    float sl = 0.9 + 0.22 * rl;
    float streak = exp(-perp * perp / (sw * sw) - along * along / (sl * sl));
    vec2 dl = P.xz - L.xz;
    float r = uLightPos[i].w;
    float pool = water ? 0.0 : exp(-dot(dl, dl) / (r * r * 0.12)) * exp(-L.y / r);
    float high = 1.0 / (1.0 + max(L.y - 5.0, 0.0) * 0.3);
    col += lc * (streak * (0.15 + 0.85 * wet) * 0.6 * high + pool * 0.22);
  }
  vec3 R = vec3(dir.x, -dir.y, dir.z);
  R.x += ripple * 0.05;
  float mr = max(dot(normalize(R), uMoon), 0.0);
  col += vec3(0.7, 0.75, 0.9) * pow(mr, water ? 180.0 : 900.0) * wet * (water ? 0.8 : 0.35);
  col += vec3(0.04, 0.05, 0.07) * pow(mr, 8.0) * wet * 0.4;
  col = col / (1.0 + col * 0.7);
  return mix(col, uFogColor, cityFog(P));
}
void main() {
  if (uWalk > 0.5) {
    vec4 a = uInvVP * vec4(vNdc, -1.0, 1.0);
    vec4 b = uInvVP * vec4(vNdc, 1.0, 1.0);
    vec3 dir = normalize(b.xyz / b.w - a.xyz / a.w);
    if (abs(dir.x) < 1e-5) dir.x = 1e-5;
    if (abs(dir.y) < 1e-5) dir.y = -1e-5;
    if (abs(dir.z) < 1e-5) dir.z = 1e-5;
    vec3 col = city(dir);
    vec2 v = vNdc * vec2(0.9, 1.0);
    col *= 1.0 - 0.45 * smoothstep(0.4, 1.7, dot(v, v));
    outColor = vec4(col, 1.0);
    return;
  }
  vec3 top = vec3(0.020, 0.043, 0.082);
  vec3 bot = vec3(0.035, 0.078, 0.140);
  vec3 col = mix(bot, top, smoothstep(-1.0, 1.0, vNdc.y));
  vec4 a = uInvVP * vec4(vNdc, -1.0, 1.0);
  vec4 b = uInvVP * vec4(vNdc, 1.0, 1.0);
  vec3 p0 = a.xyz / a.w;
  vec3 dir = normalize(b.xyz / b.w - p0);
  if (dir.y < -1e-4) {
    float t = -uEye.y / dir.y;
    vec3 hit = uEye + dir * t;
    float camD = distance(uEye, hit);
    float far = 1.0 - smoothstep(uFog.y * 0.6, uFog.y * 2.2, camD);
    float horizon = smoothstep(0.0, 0.12, -dir.y);
    float k = far * horizon;
    // Lighter blueprint paper under the installation.
    float r = distance(hit.xz, uScene.xy);
    col += vec3(0.020, 0.050, 0.090) * exp(-r / max(uScene.z, 1.0)) * k;
    float minor = grid(hit.xz, 0.1, 1.0);
    float major = grid(hit.xz, 1.0, 1.25);
    float sect = grid(hit.xz, 5.0, 1.6);
    col += vec3(0.20, 0.45, 0.70) * minor * 0.10 * k;
    col += vec3(0.30, 0.62, 0.90) * major * 0.22 * k;
    col += vec3(0.40, 0.75, 1.00) * sect * 0.18 * k;
    // Axes through the origin.
    vec2 ax = abs(hit.xz) / max(fwidth(hit.xz), vec2(1e-5));
    col += vec3(0.35, 0.7, 1.0) * (1.0 - min(min(ax.x, ax.y) / 1.2, 1.0)) * 0.12 * k;
  }
  // Vignette.
  vec2 v = vNdc * vec2(0.9, 1.0);
  col *= 1.0 - 0.35 * smoothstep(0.6, 1.6, dot(v, v));
  outColor = vec4(col, 1.0);
}
"#;

struct Prog {
    p: WebGlProgram,
    u: Vec<(&'static str, Option<WebGlUniformLocation>)>,
}

impl Prog {
    fn loc(&self, name: &str) -> Option<&WebGlUniformLocation> {
        self.u.iter().find(|(n, _)| *n == name).and_then(|(_, l)| l.as_ref())
    }
}

const UNIFORMS: [&str; 21] = [
    "uVP", "uEye", "uViewport", "uDpr", "uTime", "uMotion", "uFog", "uHidden", "uAlpha", "uDepthOnly", "uProjY", "uInvVP", "uScene", "uWalk", "uFogColor",
    "uMoon", "uLightPos", "uLightCol", "uLightCount", "uWaterX", "uLinePass",
];

pub const MOON: [f32; 3] = [0.70, 0.36, -0.62];
const FOG_COLOR: [f32; 3] = [0.050, 0.064, 0.092];

struct Batch {
    vao: WebGlVertexArrayObject,
    buf: WebGlBuffer,
    count: i32,
}

pub struct Renderer {
    gl: GL,
    canvas: HtmlCanvasElement,
    bg: Prog,
    line: Prog,
    face: Prog,
    led: Prog,
    bg_vao: WebGlVertexArrayObject,
    lines: Batch,
    faces: Batch,
    leds: Batch,
    highlight: Batch,
    dynamic: Batch,
    geo_version: u64,
    hl_version: u64,
    dyn_version: u64,
    vp: [f32; 16],
    light_pos: Vec<f32>,
    light_col: Vec<f32>,
    light_count: i32,
}

fn compile(gl: &GL, kind: u32, src: &str) -> Result<WebGlShader, String> {
    let s = gl.create_shader(kind).ok_or("cannot create shader")?;
    gl.shader_source(&s, src);
    gl.compile_shader(&s);
    if gl.get_shader_parameter(&s, GL::COMPILE_STATUS).as_bool().unwrap_or(false) {
        Ok(s)
    } else {
        Err(gl.get_shader_info_log(&s).unwrap_or_default())
    }
}

fn program(gl: &GL, vs: &str, fs: &str) -> Result<Prog, String> {
    let head = "#version 300 es\nprecision highp float;\nprecision highp int;\n";
    let v = compile(gl, GL::VERTEX_SHADER, &format!("{head}{COMMON}{vs}"))?;
    let f = compile(gl, GL::FRAGMENT_SHADER, &format!("{head}{COMMON}{fs}"))?;
    let p = gl.create_program().ok_or("cannot create program")?;
    gl.attach_shader(&p, &v);
    gl.attach_shader(&p, &f);
    gl.link_program(&p);
    if !gl.get_program_parameter(&p, GL::LINK_STATUS).as_bool().unwrap_or(false) {
        return Err(gl.get_program_info_log(&p).unwrap_or_default());
    }
    let u = UNIFORMS.iter().map(|n| (*n, gl.get_uniform_location(&p, n))).collect();
    Ok(Prog { p, u })
}

fn upload(gl: &GL, buf: &WebGlBuffer, data: &[f32], usage: u32) {
    gl.bind_buffer(GL::ARRAY_BUFFER, Some(buf));
    // SAFETY: the view is consumed immediately, before any Rust allocation can move memory.
    unsafe {
        let view = js_sys::Float32Array::view(data);
        gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER, &view, usage);
    }
}

/// Static per-vertex buffer + instance buffer with the given attribute layout (loc, size, offset in floats).
fn batch(gl: &GL, mesh: &[f32], mesh_attrs: &[(u32, i32, i32)], mesh_stride: i32, inst_attrs: &[(u32, i32, i32)], inst_stride: usize) -> Result<Batch, String> {
    let vao = gl.create_vertex_array().ok_or("cannot create VAO")?;
    gl.bind_vertex_array(Some(&vao));
    let mbuf = gl.create_buffer().ok_or("cannot create buffer")?;
    upload(gl, &mbuf, mesh, GL::STATIC_DRAW);
    for &(loc, size, off) in mesh_attrs {
        gl.enable_vertex_attrib_array(loc);
        gl.vertex_attrib_pointer_with_i32(loc, size, GL::FLOAT, false, mesh_stride * 4, off * 4);
    }
    let buf = gl.create_buffer().ok_or("cannot create buffer")?;
    gl.bind_buffer(GL::ARRAY_BUFFER, Some(&buf));
    for &(loc, size, off) in inst_attrs {
        gl.enable_vertex_attrib_array(loc);
        gl.vertex_attrib_pointer_with_i32(loc, size, GL::FLOAT, false, (inst_stride * 4) as i32, off * 4);
        gl.vertex_attrib_divisor(loc, 1);
    }
    gl.bind_vertex_array(None);
    Ok(Batch { vao, buf, count: 0 })
}

fn cube() -> Vec<f32> {
    // 6 faces × 2 triangles, position (0..1) + normal.
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        ([1., 0., 0.], [[1., 0., 0.], [1., 1., 0.], [1., 1., 1.], [1., 0., 1.]]),
        ([-1., 0., 0.], [[0., 0., 1.], [0., 1., 1.], [0., 1., 0.], [0., 0., 0.]]),
        ([0., 1., 0.], [[0., 1., 1.], [1., 1., 1.], [1., 1., 0.], [0., 1., 0.]]),
        ([0., -1., 0.], [[0., 0., 0.], [1., 0., 0.], [1., 0., 1.], [0., 0., 1.]]),
        ([0., 0., 1.], [[0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.]]),
        ([0., 0., -1.], [[1., 0., 0.], [0., 0., 0.], [0., 1., 0.], [1., 1., 0.]]),
    ];
    let mut v = Vec::with_capacity(36 * 6);
    for (n, q) in faces {
        for i in [0, 1, 2, 0, 2, 3] {
            v.extend_from_slice(&q[i]);
            v.extend_from_slice(&n);
        }
    }
    v
}

impl Renderer {
    pub fn new(canvas: &HtmlCanvasElement) -> Result<Renderer, String> {
        let opts = js_sys::Object::new();
        let set = |k: &str, v: JsValue| {
            let _ = js_sys::Reflect::set(&opts, &k.into(), &v);
        };
        set("antialias", true.into());
        set("alpha", false.into());
        set("depth", true.into());
        set("premultipliedAlpha", false.into());
        let gl: GL = canvas
            .get_context_with_context_options("webgl2", &opts)
            .map_err(|_| "WebGL2 is not available".to_string())?
            .ok_or("WebGL2 is not available in this browser")?
            .dyn_into()
            .map_err(|_| "WebGL2 is not available".to_string())?;

        let bg = program(&gl, BG_VS, BG_FS)?;
        let line = program(&gl, LINE_VS, LINE_FS)?;
        let face = program(&gl, FACE_VS, FACE_FS)?;
        let led = program(&gl, LED_VS, LED_FS)?;
        let bg_vao = gl.create_vertex_array().ok_or("cannot create VAO")?;

        let quad: [f32; 12] = [0., -1., 1., -1., 1., 1., 0., -1., 1., 1., 0., 1.];
        let line_attrs = [(1, 3, 0), (2, 3, 3), (3, 4, 6), (4, 3, 10), (5, 4, 13)];
        let lines = batch(&gl, &quad, &[(0, 2, 0)], 2, &line_attrs, LINE_STRIDE)?;
        let highlight = batch(&gl, &quad, &[(0, 2, 0)], 2, &line_attrs, LINE_STRIDE)?;
        let dynamic = batch(&gl, &quad, &[(0, 2, 0)], 2, &line_attrs, LINE_STRIDE)?;
        let faces = batch(&gl, &cube(), &[(0, 3, 0), (1, 3, 3)], 6, &[(2, 3, 0), (3, 3, 3), (4, 4, 6), (5, 3, 10), (6, 3, 13)], FACE_STRIDE)?;
        let sq: [f32; 12] = [-1., -1., 1., -1., 1., 1., -1., -1., 1., 1., -1., 1.];
        let leds = batch(&gl, &sq, &[(0, 2, 0)], 2, &[(1, 4, 0), (2, 4, 4), (3, 3, 8), (4, 4, 11), (5, 1, 15)], LED_STRIDE)?;

        Ok(Renderer {
            gl,
            canvas: canvas.clone(),
            bg,
            line,
            face,
            led,
            bg_vao,
            lines,
            faces,
            leds,
            highlight,
            dynamic,
            geo_version: u64::MAX,
            hl_version: u64::MAX,
            dyn_version: u64::MAX,
            vp: [0.0; 16],
            light_pos: vec![0.0; 48 * 4],
            light_col: vec![0.0; 48 * 4],
            light_count: 0,
        })
    }

    pub fn lost(&self) -> bool {
        self.gl.is_context_lost()
    }

    pub fn resize(&mut self, css_w: f32, css_h: f32, dpr: f32) {
        let w = (css_w * dpr).round().max(1.0) as u32;
        let h = (css_h * dpr).round().max(1.0) as u32;
        if self.canvas.width() != w {
            self.canvas.set_width(w);
        }
        if self.canvas.height() != h {
            self.canvas.set_height(h);
        }
    }

    fn common(&self, p: &Prog, e: &Engine, vp: &[f32; 16]) {
        let gl = &self.gl;
        gl.use_program(Some(&p.p));
        gl.uniform_matrix4fv_with_f32_array(p.loc("uVP"), false, vp);
        let eye = e.cam.eye();
        gl.uniform3f(p.loc("uEye"), eye.x, eye.y, eye.z);
        gl.uniform2f(p.loc("uViewport"), self.canvas.width() as f32, self.canvas.height() as f32);
        gl.uniform1f(p.loc("uDpr"), e.dpr);
        gl.uniform1f(p.loc("uTime"), (e.now % 1.0e7) as f32);
        gl.uniform1f(p.loc("uMotion"), if e.reduced_motion { 0.0 } else { 1.0 });
        let r = e.layout.bounds.map(|b| b.radius()).unwrap_or(2.0);
        let (f0, f1) = e.cam.fog(r);
        gl.uniform2f(p.loc("uFog"), f0, f1);
        let city = e.layout.room.is_some();
        gl.uniform1f(p.loc("uWalk"), if city { 1.0 } else { 0.0 });
        if city {
            gl.uniform3f(p.loc("uFogColor"), FOG_COLOR[0], FOG_COLOR[1], FOG_COLOR[2]);
            gl.uniform3f(p.loc("uMoon"), MOON[0], MOON[1], MOON[2]);
            gl.uniform1i(p.loc("uLightCount"), self.light_count);
            if self.light_count > 0 {
                gl.uniform4fv_with_f32_array(p.loc("uLightPos"), &self.light_pos);
                gl.uniform4fv_with_f32_array(p.loc("uLightCol"), &self.light_col);
            }
        } else {
            gl.uniform1i(p.loc("uLightCount"), 0);
        }
    }

    pub fn draw(&mut self, e: &Engine) {
        let gl = &self.gl;
        if self.geo_version != e.geo_version {
            self.geo_version = e.geo_version;
            let g = &e.layout.geo;
            upload(gl, &self.lines.buf, &g.lines, GL::STATIC_DRAW);
            upload(gl, &self.faces.buf, &g.faces, GL::STATIC_DRAW);
            upload(gl, &self.leds.buf, &g.leds, GL::STATIC_DRAW);
            self.lines.count = g.line_count() as i32;
            self.faces.count = g.face_count() as i32;
            self.leds.count = g.led_count() as i32;
            self.light_count = 0;
            if let Some(city) = e.layout.room.as_ref() {
                for (i, l) in city.lights.iter().take(48).enumerate() {
                    self.light_pos[i * 4..i * 4 + 4].copy_from_slice(&l[0..4]);
                    self.light_col[i * 4..i * 4 + 4].copy_from_slice(&l[4..8]);
                    self.light_count = i as i32 + 1;
                }
            }
        }
        if self.dyn_version != e.dynamic_version {
            self.dyn_version = e.dynamic_version;
            upload(gl, &self.dynamic.buf, &e.dynamic.lines, GL::DYNAMIC_DRAW);
            self.dynamic.count = e.dynamic.line_count() as i32;
        }
        if self.hl_version != e.highlight_version {
            self.hl_version = e.highlight_version;
            upload(gl, &self.highlight.buf, &e.highlight.lines, GL::DYNAMIC_DRAW);
            self.highlight.count = e.highlight.line_count() as i32;
        }

        let (w, h) = (self.canvas.width() as i32, self.canvas.height() as i32);
        gl.viewport(0, 0, w, h);
        let vpm: Mat4 = e.cam.view_proj();
        self.vp = vpm.to_cols_array();
        let vp = self.vp;
        gl.depth_mask(true);
        gl.color_mask(true, true, true, true);
        let city = e.layout.room.is_some();
        if city {
            gl.clear_color(FOG_COLOR[0], FOG_COLOR[1], FOG_COLOR[2], 1.0);
        } else {
            gl.clear_color(0.03, 0.06, 0.11, 1.0);
        }
        gl.clear(GL::COLOR_BUFFER_BIT | GL::DEPTH_BUFFER_BIT);

        // 1. Background + floor grid.
        gl.disable(GL::DEPTH_TEST);
        gl.disable(GL::BLEND);
        self.common(&self.bg, e, &vp);
        let inv = vpm.inverse().to_cols_array();
        gl.uniform_matrix4fv_with_f32_array(self.bg.loc("uInvVP"), false, &inv);
        let (c, r) = e.layout.bounds.map(|b| (b.center(), b.radius())).unwrap_or((glam::Vec3::ZERO, 3.0));
        gl.uniform4f(self.bg.loc("uScene"), c.x, c.z, r * 1.2, 0.0);
        if let Some(c) = e.layout.room.as_ref() {
            gl.uniform1f(self.bg.loc("uWaterX"), c.water_x);
        }
        gl.bind_vertex_array(Some(&self.bg_vao));
        gl.draw_arrays(GL::TRIANGLES, 0, 3);

        // 2. Depth pre-pass from occluding faces (pushed back so their own edges stay visible).
        gl.enable(GL::DEPTH_TEST);
        gl.depth_func(GL::LEQUAL);
        gl.enable(GL::POLYGON_OFFSET_FILL);
        gl.polygon_offset(1.5, 4.0);
        if self.faces.count > 0 {
            gl.color_mask(false, false, false, false);
            self.common(&self.face, e, &vp);
            gl.uniform1f(self.face.loc("uDepthOnly"), 1.0);
            gl.bind_vertex_array(Some(&self.faces.vao));
            gl.draw_arrays_instanced(GL::TRIANGLES, 0, 36, self.faces.count);
            gl.color_mask(true, true, true, true);

            // 3. Translucent faces.
            gl.depth_mask(false);
            gl.enable(GL::BLEND);
            gl.blend_func(GL::SRC_ALPHA, GL::ONE_MINUS_SRC_ALPHA);
            gl.uniform1f(self.face.loc("uDepthOnly"), 0.0);
            gl.draw_arrays_instanced(GL::TRIANGLES, 0, 36, self.faces.count);
        }
        gl.disable(GL::POLYGON_OFFSET_FILL);
        gl.depth_mask(false);
        gl.enable(GL::BLEND);
        gl.blend_func(GL::SRC_ALPHA, GL::ONE);

        // 4. City: ink lines, then glowing ones (neon, keys, cracked glass) added on top.
        if city && self.lines.count > 0 {
            self.common(&self.line, e, &vp);
            gl.bind_vertex_array(Some(&self.lines.vao));
            gl.depth_func(GL::LEQUAL);
            gl.uniform1f(self.line.loc("uHidden"), 0.0);
            gl.uniform1f(self.line.loc("uAlpha"), 1.0);
            gl.blend_func(GL::SRC_ALPHA, GL::ONE_MINUS_SRC_ALPHA);
            gl.uniform1f(self.line.loc("uLinePass"), 1.0);
            gl.draw_arrays_instanced(GL::TRIANGLES, 0, 6, self.lines.count);
            gl.blend_func(GL::SRC_ALPHA, GL::ONE);
            gl.uniform1f(self.line.loc("uLinePass"), 2.0);
            gl.draw_arrays_instanced(GL::TRIANGLES, 0, 6, self.lines.count);
        }
        // 4. Blueprint lines: hidden (dashed, faint) then visible.
        if !city && self.lines.count > 0 {
            self.common(&self.line, e, &vp);
            gl.bind_vertex_array(Some(&self.lines.vao));
            gl.uniform1f(self.line.loc("uLinePass"), 0.0);
            gl.depth_func(GL::GREATER);
            gl.uniform1f(self.line.loc("uHidden"), 1.0);
            gl.uniform1f(self.line.loc("uAlpha"), 0.16);
            gl.draw_arrays_instanced(GL::TRIANGLES, 0, 6, self.lines.count);
            gl.depth_func(GL::LEQUAL);
            gl.uniform1f(self.line.loc("uHidden"), 0.0);
            gl.uniform1f(self.line.loc("uAlpha"), 1.0);
            gl.draw_arrays_instanced(GL::TRIANGLES, 0, 6, self.lines.count);
        }

        // 5. LEDs (additive glow, occluded by solid parts).
        if self.leds.count > 0 {
            self.common(&self.led, e, &vp);
            let proj = e.cam.proj();
            gl.uniform1f(self.led.loc("uProjY"), proj.y_axis.y);
            gl.blend_func(GL::ONE, GL::ONE_MINUS_SRC_ALPHA);
            gl.bind_vertex_array(Some(&self.leds.vao));
            gl.draw_arrays_instanced(GL::TRIANGLES, 0, 6, self.leds.count);
        }

        // 5b. Searchlight beams (additive, behind buildings).
        if city && self.dynamic.count > 0 {
            self.common(&self.line, e, &vp);
            gl.blend_func(GL::SRC_ALPHA, GL::ONE);
            gl.uniform1f(self.line.loc("uHidden"), 0.0);
            gl.uniform1f(self.line.loc("uAlpha"), 1.0);
            gl.uniform1f(self.line.loc("uLinePass"), 0.0);
            gl.bind_vertex_array(Some(&self.dynamic.vao));
            gl.draw_arrays_instanced(GL::TRIANGLES, 0, 6, self.dynamic.count);
        }

        // 6. Hover / selection outlines on top of everything.
        if self.highlight.count > 0 {
            gl.disable(GL::DEPTH_TEST);
            gl.blend_func(GL::SRC_ALPHA, GL::ONE_MINUS_SRC_ALPHA);
            self.common(&self.line, e, &vp);
            gl.uniform1f(self.line.loc("uHidden"), 0.0);
            gl.uniform1f(self.line.loc("uAlpha"), 1.0);
            gl.uniform1f(self.line.loc("uLinePass"), 0.0);
            gl.bind_vertex_array(Some(&self.highlight.vao));
            gl.draw_arrays_instanced(GL::TRIANGLES, 0, 6, self.highlight.count);
        }
        gl.bind_vertex_array(None);
    }
}
