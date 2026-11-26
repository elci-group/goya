//! The live sky runtime for `sky: local` (`sky.js`), generic across apps.
//! It is emitted by Rust, like `app.js` and `gvi.js`; no app ships its own
//! script.
//!
//! **Where and when.** The device's IANA time zone
//! (`Intl.DateTimeFormat().resolvedOptions().timeZone`) is looked up in a
//! table of representative coordinates built from tzdata (`zone1970.tab`,
//! `zone.tab`, and every link in `tzdata.zi`, so ICU's legacy names such as
//! `Asia/Calcutta` resolve). A zone with no coordinates (`UTC`, `Etc/*`)
//! falls back to the longitude implied by the UTC offset at 35° N. The
//! clock is the device's.
//!
//! **What.** The Sun's position comes from the NOAA solar equations; the
//! Moon's from a low-precision lunar ephemeris (about 1°) with its phase.
//! The frame looks towards the equator, 22° above the horizon, with a 60°
//! vertical field. Four layers, each drawn at the resolution and rate its
//! content needs (a TV GPU such as the PowerVR BXE-4-32 cannot shade a
//! procedural sky at full resolution in real time):
//! 1. **Atmosphere**: single scattering (Rayleigh, Mie, ozone, Earth's
//!    shadow) with a blue multiple-scattering fill, average-metered
//!    exposure and an ACES curve. WebGL at 320×180 every 5 s; the field is
//!    smooth, so upscaling loses nothing.
//! 2. **Lights**: the Sun's disk and glare, the Moon with its true phase,
//!    bright limb towards the Sun, and maria, and about 1,500 stars turned
//!    by local sidereal time and dimmed by airmass. 2D canvas at the
//!    node's full size, every 5 s.
//! 3. **Clouds**: two layers on a 2 km deck that drift with the wind and
//!    slowly change shape. They are lit by the light that reaches them,
//!    self-shadowed, with silver linings towards the light. Every
//!    animation frame (at most 30 per second), from a tileable noise
//!    texture baked once on the GPU, so a frame costs a few texture reads
//!    per pixel. The resolution adapts to the frame rate the device
//!    achieves.
//! 4. **Hills**: the horizon line in front of the clouds, as full-resolution
//!    SVG, coloured by the airlight and the light on the land.
//!
//! Without WebGL, the atmosphere comes from the same model on the CPU at
//! 64×36, without clouds; lights and hills are unchanged. Nothing is drawn
//! while the document is hidden. Under `prefers-reduced-motion: reduce`,
//! the clouds hold still and everything redraws once a minute.
//!
//! The sky is decoration: it reads the clock and writes no state. Text may
//! never sit on it (`GY-AR-622`, see `motion.rs`).

pub const CONTRACT: &str = "arachnid.sky/0.1.0";
pub const FILE: &str = "sky.js";
/// Height of the atmosphere canvas; its width follows the node's aspect.
pub const BASE_HEIGHT: u32 = 180;

const ZONES: &str = include_str!("../data/tz-coordinates.tsv");

/// The zone table as a JS object literal, checked while it is built.
pub fn zones_js() -> Result<String, String> {
    let mut out = String::from("{");
    let mut first = true;
    for (n, line) in ZONES.lines().enumerate() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        let parse = |s: &str| s.parse::<f64>().ok().filter(|v| v.is_finite());
        let ok = f.len() == 3
            && !f[0].is_empty()
            && f[0]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/_-+".contains(&b));
        match (
            ok,
            parse(f.get(1).copied().unwrap_or("")),
            parse(f.get(2).copied().unwrap_or("")),
        ) {
            (true, Some(lat), Some(lon)) if lat.abs() <= 90.0 && lon.abs() <= 180.0 => {
                if !first {
                    out.push(',');
                }
                first = false;
                out.push_str(&format!("\"{}\":[{lat},{lon}]", f[0]));
            }
            _ => return Err(format!("tz-coordinates.tsv line {}: {line:?}", n + 1)),
        }
    }
    out.push('}');
    Ok(out)
}

/// The generated `sky.js`.
pub fn script() -> String {
    let zones = zones_js().expect("the vendored zone table is checked by tests");
    SKY_JS.replace("/*ZONES*/{}", &zones)
}

/// The sky layers placed first inside the sky node, whose resolved size is
/// `width`×`height` CSS px.
pub fn canvas_markup(width: u32, height: u32) -> String {
    let (w, h) = (width.max(1), height.max(1));
    let base_w = ((BASE_HEIGHT as u64 * w as u64 + h as u64 / 2) / h as u64).max(1);
    format!(
        "<canvas class=\"sky\" width=\"{base_w}\" height=\"{BASE_HEIGHT}\" aria-hidden=\"true\"></canvas>\
<canvas class=\"sky-lights\" width=\"{w}\" height=\"{h}\" aria-hidden=\"true\"></canvas>\
<canvas class=\"sky-clouds\" width=\"{w}\" height=\"{h}\" aria-hidden=\"true\"></canvas>\
<svg class=\"sky-hills\" viewBox=\"0 0 {w} {h}\" preserveAspectRatio=\"none\" aria-hidden=\"true\">\
<defs><linearGradient id=\"sky-hill-air\" x1=\"0\" y1=\"0\" x2=\"1\" y2=\"0\"></linearGradient>\
<linearGradient id=\"sky-hill-depth\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"1\"></linearGradient></defs>\
<path fill=\"url(#sky-hill-air)\"></path><path fill=\"url(#sky-hill-depth)\"></path></svg>"
    )
}

const SKY_JS: &str = r##"/* Goya live sky (arachnid.sky/0.1.0), generated by Arachnid. */
(function (root) {
  "use strict";
  var ZONES = /*ZONES*/{};
  var RAD = Math.PI / 180;
  var FACE_PITCH = 22 * RAD, VFOV = 60 * RAD, ASPECT = 16 / 9;

  function jd(ms) { return ms / 86400000 + 2440587.5; }
  function gmst(j) {
    var T = (j - 2451545) / 36525;
    var g = 280.46061837 + 360.98564736629 * (j - 2451545) + T * T * (0.000387933 - T / 38710000);
    return ((g % 360) + 360) % 360;
  }
  /* Equatorial (radians) to horizontal: altitude, and azimuth from north
     through east. */
  function horizontal(ra, dec, j, lat, lon) {
    var H0 = (gmst(j) + lon) * RAD - ra, phi = lat * RAD;
    var alt = Math.asin(Math.sin(phi) * Math.sin(dec) + Math.cos(phi) * Math.cos(dec) * Math.cos(H0));
    var az = Math.atan2(-Math.sin(H0), Math.tan(dec) * Math.cos(phi) - Math.sin(phi) * Math.cos(H0));
    return { alt: alt, az: (az + 2 * Math.PI) % (2 * Math.PI) };
  }
  /* NOAA solar position. */
  function sun(ms, lat, lon) {
    var j = jd(ms), T = (j - 2451545) / 36525;
    var L0 = 280.46646 + T * (36000.76983 + T * 0.0003032);
    var M = (357.52911 + T * (35999.05029 - 0.0001537 * T)) * RAD;
    var C = Math.sin(M) * (1.914602 - T * (0.004817 + 0.000014 * T)) + Math.sin(2 * M) * (0.019993 - 0.000101 * T) + Math.sin(3 * M) * 0.000289;
    var om = (125.04 - 1934.136 * T) * RAD;
    var lam = (L0 + C - 0.00569 - 0.00478 * Math.sin(om)) * RAD;
    var eps = (23 + (26 + (21.448 - T * (46.815 + T * (0.00059 - T * 0.001813))) / 60) / 60 + 0.00256 * Math.cos(om)) * RAD;
    var ra = Math.atan2(Math.cos(eps) * Math.sin(lam), Math.cos(lam));
    var dec = Math.asin(Math.sin(eps) * Math.sin(lam));
    return horizontal(ra, dec, j, lat, lon);
  }
  /* Low-precision Moon (about 1 degree), with its illuminated fraction. */
  function moon(ms, lat, lon) {
    var j = jd(ms), d = j - 2451545;
    var L = (218.316 + 13.176396 * d) * RAD, M = (134.963 + 13.064993 * d) * RAD, F = (93.272 + 13.22935 * d) * RAD;
    var l = L + 6.289 * RAD * Math.sin(M), b = 5.128 * RAD * Math.sin(F), e = 23.4397 * RAD;
    var ra = Math.atan2(Math.sin(l) * Math.cos(e) - Math.tan(b) * Math.sin(e), Math.cos(l));
    var dec = Math.asin(Math.sin(b) * Math.cos(e) + Math.cos(b) * Math.sin(e) * Math.sin(l));
    var h = horizontal(ra, dec, j, lat, lon);
    var Ms = (357.529 + 0.98560028 * d) * RAD;
    var ls = (280.459 + 0.98564736 * d) * RAD + (1.915 * Math.sin(Ms) + 0.02 * Math.sin(2 * Ms)) * RAD;
    var elong = Math.acos(Math.cos(b) * Math.cos(l - ls));
    h.illum = (1 - Math.cos(elong)) / 2;
    return h;
  }
  /* Representative coordinates for an IANA zone, else from the offset. */
  function locate(zone, offsetMinutes) {
    var z = zone && Object.prototype.hasOwnProperty.call(ZONES, zone) ? ZONES[zone] : null;
    if (z) return { lat: z[0], lon: z[1], zone: zone, source: "zone" };
    var lon = Math.max(-180, Math.min(180, -offsetMinutes / 4));
    return { lat: 35, lon: lon, zone: zone || null, source: "offset" };
  }
  function vec(h) {
    var c = Math.cos(h.alt);
    return [c * Math.sin(h.az), Math.sin(h.alt), c * Math.cos(h.az)];
  }

  /* The atmosphere model, shared by the CPU path, the meter and the tests:
     single scattering, light dir l, view dir v (y up), linear radiance. */
  var RE = 6360e3, RA = 6420e3, BR = [5.802e-6, 13.558e-6, 33.1e-6], BM = 3.996e-6, EM = 4.40e-6, BO = [0.65e-6, 1.881e-6, 0.085e-6];
  function far(o, d, r) {
    var b = o[0] * d[0] + o[1] * d[1] + o[2] * d[2], c = o[0] * o[0] + o[1] * o[1] + o[2] * o[2] - r * r, q = b * b - c;
    return q < 0 ? -1 : -b + Math.sqrt(q);
  }
  function dens(hh) { return [Math.exp(-hh / 8000), Math.exp(-hh / 1200), Math.max(0, 1 - Math.abs(hh - 25000) / 15000)]; }
  function ext(od, k) { return Math.exp(-(BR[k] * od[0] + EM * od[1] + BO[k] * od[2])); }
  function radiance(v, l, nv, nl) {
    nv = nv || 8; nl = nl || 4;
    var o = [0, RE + 200, 0], t = far(o, v, RA);
    if (v[1] < 0) {
      var b = o[1] * v[1], c = o[1] * o[1] - RE * RE, q = b * b - c;
      if (q > 0) { var tg = -b - Math.sqrt(q); if (tg > 0) t = Math.min(t, tg); }
    }
    var ds = t / nv, od = [0, 0, 0], sr = [0, 0, 0], sm = [0, 0, 0];
    for (var i = 0; i < nv; i++) {
      var p = [o[0] + v[0] * ds * (i + 0.5), o[1] + v[1] * ds * (i + 0.5), o[2] + v[2] * ds * (i + 0.5)];
      var hh = Math.sqrt(p[0] * p[0] + p[1] * p[1] + p[2] * p[2]) - RE, dn = dens(hh);
      od[0] += dn[0] * ds; od[1] += dn[1] * ds; od[2] += dn[2] * ds;
      var tl = far(p, l, RA), dl = tl / nl, ol = [0, 0, 0], shadow = false;
      for (var k = 0; k < nl; k++) {
        var qp = [p[0] + l[0] * dl * (k + 0.5), p[1] + l[1] * dl * (k + 0.5), p[2] + l[2] * dl * (k + 0.5)];
        var qh = Math.sqrt(qp[0] * qp[0] + qp[1] * qp[1] + qp[2] * qp[2]) - RE;
        if (qh < 0) { shadow = true; break; }
        var qd = dens(qh); ol[0] += qd[0] * dl; ol[1] += qd[1] * dl; ol[2] += qd[2] * dl;
      }
      if (shadow) continue;
      for (var ch = 0; ch < 3; ch++) {
        var T = ext([od[0] + ol[0], od[1] + ol[1], od[2] + ol[2]], ch);
        sr[ch] += T * dn[0] * ds; sm[ch] += T * dn[1] * ds;
      }
    }
    var mu = v[0] * l[0] + v[1] * l[1] + v[2] * l[2], g = 0.76;
    var pr = 3 / (16 * Math.PI) * (1 + mu * mu);
    var pm = 3 / (8 * Math.PI) * ((1 - g * g) * (1 + mu * mu)) / ((2 + g * g) * Math.pow(1 + g * g - 2 * g * mu, 1.5));
    return [0, 1, 2].map(function (ch) { return 22 * (sr[ch] * BR[ch] * pr + sm[ch] * BM * pm); });
  }
  /* Transmittance from height hh (m) towards l, per channel. */
  function transmit(l, hh) {
    var p = [0, RE + hh, 0], t = far(p, l, RA), dl = t / 8, od = [0, 0, 0];
    for (var k = 0; k < 8; k++) {
      var q = [l[0] * dl * (k + 0.5), RE + hh + l[1] * dl * (k + 0.5), l[2] * dl * (k + 0.5)];
      var qh = Math.sqrt(q[0] * q[0] + q[1] * q[1] + q[2] * q[2]) - RE;
      if (qh < 0) return [0, 0, 0];
      var d = dens(qh); od[0] += d[0] * dl; od[1] += d[1] * dl; od[2] += d[2] * dl;
    }
    return [ext(od, 0), ext(od, 1), ext(od, 2)];
  }
  function luminance(c) { return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]; }
  /* The camera: a screen point q (frame units, y up) to a world direction,
     and back. */
  function basis(face) {
    var cp = Math.cos(FACE_PITCH), sp = Math.sin(FACE_PITCH);
    var fw0 = [Math.sin(face), 0, Math.cos(face)], rt = [Math.cos(face), 0, -Math.sin(face)];
    return { rt: rt, up: [-fw0[0] * sp, cp, -fw0[2] * sp], fw: [fw0[0] * cp, sp, fw0[2] * cp] };
  }
  function view(qx, qy, face) {
    var f = 1 / Math.tan(VFOV / 2), B = basis(face), n = Math.sqrt(qx * qx + qy * qy + f * f);
    var cx = qx / n, cy = qy / n, cz = f / n;
    return [B.rt[0] * cx + B.up[0] * cy + B.fw[0] * cz, B.up[1] * cy + B.fw[1] * cz, B.rt[2] * cx + B.up[2] * cy + B.fw[2] * cz];
  }
  /* World direction to a point in a w x h frame, or null behind the camera. */
  function project(v, face, w, h) {
    var B = basis(face), f = 1 / Math.tan(VFOV / 2);
    var cz = v[0] * B.fw[0] + v[1] * B.fw[1] + v[2] * B.fw[2];
    if (cz <= 0.01) return null;
    var cx = v[0] * B.rt[0] + v[2] * B.rt[2], cy = v[0] * B.up[0] + v[1] * B.up[1] + v[2] * B.up[2];
    return [(cx / cz * f / (w / h) + 1) / 2 * w, (1 - cy / cz * f) / 2 * h];
  }
  function aces(x) { return Math.max(0, Math.min(1, (x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14))); }
  function display(x) { return Math.round(255 * Math.pow(aces(x), 1 / 2.2)); }
  function cloudCover(ms) {
    var day = Math.floor(ms / 86400000), x = Math.sin(day * 12.9898) * 43758.5453;
    return 0.5 + 0.1 * (x - Math.floor(x));
  }
  /* The hill line's elevation (radians) at azimuth az. */
  function hill(az) {
    return 0.017 + 0.006 * Math.sin(az * 5 + 1.3) + 0.004 * Math.sin(az * 11 + 0.7) + 0.002 * Math.sin(az * 23 + 2.1) +
      0.0012 * Math.sin(az * 47 + 4.0) + 0.0006 * Math.sin(az * 97);
  }

  /* The light the sky is drawn with: the Sun until it is 12 degrees down,
     then the Moon. Exposure follows it, as a camera meters. */
  function lighting(ms, where) {
    var s = sun(ms, where.lat, where.lon), m = moon(ms, where.lat, where.lon);
    var sd = s.alt / RAD, useSun = sd > -12;
    var light = useSun ? s : m, intensity = useSun ? 1 : 2.5e-6 * Math.max(0.05, m.illum) * (m.alt > 0 ? 1 : 0.15);
    /* Average metering over a 4x3 grid of directions in the frame. */
    var face = where.lat >= 0 ? Math.PI : 0, lv = vec(light), sum = 0, nq = 0;
    for (var gx = 0; gx < 4; gx++) for (var gy = 0; gy < 3; gy++) {
      sum += luminance(radiance(view(-1.3 + gx * 0.866, 0.7 - gy * 0.55, face), lv, 6, 3));
      nq++;
    }
    var lum = sum / nq * intensity;
    /* Multiple scattering, which single scattering lacks: a faint
       blue-weighted fill in proportion to the light in the frame. */
    var ms2 = [0.10 * lum, 0.16 * lum, 0.30 * lum];
    /* The meter may not chase a vanishing sky: the cap rises gently, and a
       display-referred navy floor (airglow, starlight) keeps night blue. */
    var cap = useSun ? Math.pow(10, 2.3 + Math.max(0, -sd - 5) * 0.22) : 2e6;
    var target = useSun
      ? (sd >= 10 ? 0.5 : sd >= 0 ? 0.44 + 0.006 * sd : sd >= -6 ? 0.26 + 0.03 * (sd + 6) : 0.1 + 0.027 * (sd + 12))
      : 0.026 + (m.alt > 0 ? 0.026 * m.illum : 0);
    var exposure = Math.min(cap, target / Math.max(lum * 1.25, 1e-14));
    var floor = 0.028 / exposure;
    ms2 = [ms2[0] + 0.45 * floor, ms2[1] + 0.75 * floor, ms2[2] + 1.6 * floor];
    var night = Math.max(0, Math.min(1, (-sd - 6) / 8));
    return { sun: s, moon: m, light: light, intensity: intensity, exposure: exposure, night: night, useSun: useSun, ms: ms2, face: face };
  }

  var VS = "attribute vec2 p; void main() { gl_Position = vec4(p, 0.0, 1.0); }";
  var CAMERA = [
    "vec3 camera(vec2 frag, vec2 res, float face) {",
    "  vec2 q = frag / res * 2.0 - 1.0; q.x *= res.x / res.y;",
    "  float f = 1.0 / tan(" + (VFOV / 2).toFixed(6) + ");",
    "  vec3 c = normalize(vec3(q.x, q.y, f)); float cp = cos(" + FACE_PITCH.toFixed(6) + "); float sp = sin(" + FACE_PITCH.toFixed(6) + ");",
    "  c = vec3(c.x, c.y * cp + c.z * sp, -c.y * sp + c.z * cp);",
    "  vec3 fw = vec3(sin(face), 0.0, cos(face)); vec3 rt = vec3(cos(face), 0.0, -sin(face));",
    "  return normalize(rt * c.x + vec3(0.0, c.y, 0.0) + fw * c.z);",
    "}"
  ].join("\n");
  var TONE = "vec3 tone(vec3 x) { x = clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0); return pow(x, vec3(1.0 / 2.2)); }";
  /* Layer 1: the atmosphere, exposed and tone-mapped. Below the horizon it
     shows the airlight just above it (the hills cover it). */
  var FS_AIR = [
    "precision highp float;",
    "uniform vec2 uRes; uniform vec3 uLight; uniform float uIntensity; uniform float uExposure; uniform float uFace; uniform vec3 uMS;",
    "const float RE = 6360e3; const float RA = 6420e3; const float PI = 3.14159265;",
    "const vec3 BR = vec3(5.802e-6, 13.558e-6, 33.1e-6); const float BM = 3.996e-6; const float EM = 4.40e-6;",
    "const vec3 BO = vec3(0.65e-6, 1.881e-6, 0.085e-6);",
    "float far(vec3 o, vec3 d, float r) { float b = dot(o, d); float c = dot(o, o) - r * r; float q = b * b - c; return q < 0.0 ? -1.0 : -b + sqrt(q); }",
    "vec3 dens(float h) { return vec3(exp(-h / 8000.0), exp(-h / 1200.0), max(0.0, 1.0 - abs(h - 25000.0) / 15000.0)); }",
    "vec3 ext(vec3 od) { return exp(-(BR * od.x + EM * od.y + BO * od.z)); }",
    "vec3 toLight(vec3 p, vec3 l) {",
    "  float dl = far(p, l, RA) / 6.0; vec3 od = vec3(0.0);",
    "  for (int k = 0; k < 6; k++) { vec3 q = p + l * (dl * (float(k) + 0.5)); float h = length(q) - RE; if (h < 0.0) return vec3(1e9); od += dens(h) * dl; }",
    "  return od;",
    "}",
    CAMERA, TONE,
    "void main() {",
    "  vec3 v = camera(gl_FragCoord.xy, uRes, uFace);",
    "  v = normalize(vec3(v.x, max(v.y, 0.004), v.z));",
    "  vec3 o = vec3(0.0, RE + 200.0, 0.0); float t = far(o, v, RA);",
    "  float ds = t / 12.0; vec3 od = vec3(0.0); vec3 sr = vec3(0.0); vec3 sm = vec3(0.0);",
    "  for (int i = 0; i < 12; i++) {",
    "    vec3 p = o + v * (ds * (float(i) + 0.5)); vec3 dn = dens(length(p) - RE); od += dn * ds;",
    "    vec3 T = ext(od + toLight(p, uLight)); sr += T * dn.x * ds; sm += T * dn.y * ds;",
    "  }",
    "  float mu = dot(v, uLight); float g = 0.76;",
    "  float pr = 3.0 / (16.0 * PI) * (1.0 + mu * mu);",
    "  float pm = 3.0 / (8.0 * PI) * ((1.0 - g * g) * (1.0 + mu * mu)) / ((2.0 + g * g) * pow(1.0 + g * g - 2.0 * g * mu, 1.5));",
    "  vec3 L = 22.0 * (sr * BR * pr + sm * BM * pm) * uIntensity + uMS * (0.7 + 0.3 * v.y);",
    "  gl_FragColor = vec4(tone(L * uExposure), 1.0);",
    "}"
  ].join("\n");
  /* Bakes a tileable noise texture once: R and G two value-noise fbm
     fields, B the low-passed R (for self-shadowing), A fine detail. */
  var FS_NOISE = [
    "precision highp float;",
    "uniform float uSize;",
    "float hash(vec2 p) { p = fract(p * vec2(123.34, 456.21)); p += dot(p, p + 45.32); return fract(p.x * p.y); }",
    "float vn(vec2 p, float P, float seed) { vec2 i = floor(p); vec2 f = fract(p); f = f * f * (3.0 - 2.0 * f);",
    "  vec2 s = vec2(seed * 17.0, seed * 31.0);",
    "  return mix(mix(hash(mod(i, P) + s), hash(mod(i + vec2(1.0, 0.0), P) + s), f.x), mix(hash(mod(i + vec2(0.0, 1.0), P) + s), hash(mod(i + vec2(1.0, 1.0), P) + s), f.x), f.y); }",
    "float fbm(vec2 uv, float base, float seed, int oct) { float a = 0.5; float s = 0.0; float P = base; vec2 p = uv * base;",
    "  for (int i = 0; i < 8; i++) { if (i >= oct) break; s += a * vn(p, P, seed + float(i)); p *= 2.0; P *= 2.0; a *= 0.5; } return s; }",
    "void main() {",
    "  vec2 uv = gl_FragCoord.xy / uSize;",
    "  gl_FragColor = vec4(fbm(uv, 4.0, 1.0, 7), fbm(uv, 4.0, 9.0, 7), fbm(uv, 4.0, 1.0, 3), fbm(uv, 16.0, 21.0, 5));",
    "}"
  ].join("\n");
  /* Layer 3: clouds, premultiplied alpha. Lights are already exposed. */
  var FS_CLOUD = [
    "precision highp float;",
    "uniform vec2 uRes; uniform sampler2D uNoise; uniform float uTime; uniform float uCover; uniform float uFace;",
    "uniform vec3 uLight; uniform vec3 uCloudLight; uniform vec3 uAmb;",
    CAMERA, TONE,
    "void main() {",
    "  vec3 v = camera(gl_FragCoord.xy, uRes, uFace);",
    "  if (v.y < 0.015) { gl_FragColor = vec4(0.0); return; }",
    "  float t = 2000.0 / v.y; vec2 p = v.xz * t / 11000.0;",
    "  vec2 w1 = vec2(uTime * 0.0016, uTime * 0.0005); vec2 w2 = vec2(uTime * 0.0027, -uTime * 0.0004);",
    "  vec4 a = texture2D(uNoise, p + w1);",
    "  float morph = 0.5 + 0.5 * sin(uTime * 0.011);",
    "  float n = mix(a.r, a.g, morph) * 0.74 + texture2D(uNoise, p * 3.1 + w2).a * 0.26;",
    "  float d = smoothstep(uCover, uCover + 0.17, n);",
    "  float fade = smoothstep(0.015, 0.16, v.y) * exp(-t / 38000.0);",
    "  float alpha = d * fade * 0.97;",
    "  if (alpha < 0.004) { gl_FragColor = vec4(0.0); return; }",
    "  float sh = texture2D(uNoise, p + w1 + uLight.xz * 0.03).b;",
    "  float shadow = exp(-max(sh - uCover + 0.12, 0.0) * 7.0);",
    "  float mu = max(dot(v, uLight), 0.0);",
    "  float silver = d * (1.0 - d) * 4.0 * pow(mu, 5.0) * 1.6;",
    "  vec3 col = uCloudLight * ((0.16 + 0.84 * shadow) * (1.0 + 1.6 * pow(mu, 9.0)) + silver) + uAmb * (0.85 + 0.55 * d);",
    "  gl_FragColor = vec4(tone(col) * alpha, alpha);",
    "}"
  ].join("\n");

  var state = { zone: null, where: null, mode: null, frames: 0, cloudFrames: 0, pinned: null, error: null, lighting: null, fps: 0, scale: 0 };
  var node = null, air = null, lightsCv = null, cloudsCv = null, hillsSvg = null;
  var gl = null, cgl = null, P = {}, C = {}, noiseTex = null, ctx2d = null;
  var lit = null, slowTimer = null, raf = null, lastFrame = 0, frameTimes = [], scale = 0.62, startMs = Date.now();

  function now() { return state.pinned !== null ? state.pinned : Date.now(); }
  function currentZone() {
    try { return Intl.DateTimeFormat().resolvedOptions().timeZone || null; } catch (e) { return null; }
  }
  function reduced() {
    try { return !!(root.matchMedia && root.matchMedia("(prefers-reduced-motion: reduce)").matches); } catch (e) { return false; }
  }
  function program(g, fs, names) {
    function sh(type, src) {
      var s = g.createShader(type); g.shaderSource(s, src); g.compileShader(s);
      if (!g.getShaderParameter(s, g.COMPILE_STATUS)) throw new Error(g.getShaderInfoLog(s) || "shader");
      return s;
    }
    var p = g.createProgram();
    g.attachShader(p, sh(g.VERTEX_SHADER, VS)); g.attachShader(p, sh(g.FRAGMENT_SHADER, fs));
    g.linkProgram(p);
    if (!g.getProgramParameter(p, g.LINK_STATUS)) throw new Error(g.getProgramInfoLog(p) || "link");
    var u = { prog: p, loc: g.getAttribLocation(p, "p") };
    names.forEach(function (n) { u[n] = g.getUniformLocation(p, n); });
    return u;
  }
  function triangle(g) {
    var buf = g.createBuffer();
    g.bindBuffer(g.ARRAY_BUFFER, buf);
    g.bufferData(g.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), g.STATIC_DRAW);
  }
  function use(g, u) {
    g.useProgram(u.prog);
    g.enableVertexAttribArray(u.loc);
    g.vertexAttribPointer(u.loc, 2, g.FLOAT, false, 0, 0);
  }
  function highp(g) {
    var hp = g.getShaderPrecisionFormat && g.getShaderPrecisionFormat(g.FRAGMENT_SHADER, g.HIGH_FLOAT);
    return !!hp && hp.precision >= 16;
  }
  function initGL() {
    try {
      var opts = { antialias: false, depth: false, stencil: false, preserveDrawingBuffer: false, powerPreference: "low-power" };
      gl = air.getContext("webgl", Object.assign({ alpha: false }, opts)) || air.getContext("experimental-webgl");
      if (!gl || !highp(gl)) { state.error = gl ? "no highp in fragment shaders" : "no WebGL"; gl = null; return false; }
      triangle(gl);
      P = program(gl, FS_AIR, ["uRes", "uLight", "uIntensity", "uExposure", "uFace", "uMS"]);
      cgl = cloudsCv.getContext("webgl", Object.assign({ alpha: true, premultipliedAlpha: true }, opts));
      if (cgl && highp(cgl)) {
        triangle(cgl);
        bakeNoise(cgl);
        C = program(cgl, FS_CLOUD, ["uRes", "uNoise", "uTime", "uCover", "uFace", "uLight", "uCloudLight", "uAmb"]);
      } else cgl = null;
      return true;
    } catch (e) {
      state.error = String(e && e.message || e);
      gl = null; cgl = null;
      return false;
    }
  }
  function bakeNoise(g) {
    var size = 512, N = program(g, FS_NOISE, ["uSize"]);
    noiseTex = g.createTexture();
    g.bindTexture(g.TEXTURE_2D, noiseTex);
    g.texImage2D(g.TEXTURE_2D, 0, g.RGBA, size, size, 0, g.RGBA, g.UNSIGNED_BYTE, null);
    var fb = g.createFramebuffer();
    g.bindFramebuffer(g.FRAMEBUFFER, fb);
    g.framebufferTexture2D(g.FRAMEBUFFER, g.COLOR_ATTACHMENT0, g.TEXTURE_2D, noiseTex, 0);
    g.viewport(0, 0, size, size);
    use(g, N);
    g.uniform1f(N.uSize, size);
    g.drawArrays(g.TRIANGLES, 0, 3);
    g.bindFramebuffer(g.FRAMEBUFFER, null);
    g.deleteFramebuffer(fb);
    g.generateMipmap(g.TEXTURE_2D);
    g.texParameteri(g.TEXTURE_2D, g.TEXTURE_MIN_FILTER, g.LINEAR_MIPMAP_LINEAR);
    g.texParameteri(g.TEXTURE_2D, g.TEXTURE_MAG_FILTER, g.LINEAR);
    g.texParameteri(g.TEXTURE_2D, g.TEXTURE_WRAP_S, g.REPEAT);
    g.texParameteri(g.TEXTURE_2D, g.TEXTURE_WRAP_T, g.REPEAT);
  }

  /* Layer 1 on the CPU, for devices without WebGL. */
  function drawAirFlat() {
    var cw = 64, chh = 36, img = ctx2d.createImageData(cw, chh), l = vec(lit.light);
    for (var y = 0; y < chh; y++) for (var x = 0; x < cw; x++) {
      var v = view(((x + 0.5) / cw * 2 - 1) * ASPECT, 1 - (y + 0.5) / chh * 2, lit.face);
      if (v[1] < 0.004) { var n = Math.sqrt(v[0] * v[0] + 0.004 * 0.004 + v[2] * v[2]); v = [v[0] / n, 0.004 / n, v[2] / n]; }
      var L = radiance(v, l, 6, 3), o = (y * cw + x) * 4;
      for (var k = 0; k < 3; k++) img.data[o + k] = display((L[k] * lit.intensity + lit.ms[k] * (0.7 + 0.3 * v[1])) * lit.exposure);
      img.data[o + 3] = 255;
    }
    var tmp = document.createElement("canvas"); tmp.width = cw; tmp.height = chh;
    tmp.getContext("2d").putImageData(img, 0, 0);
    ctx2d.imageSmoothingEnabled = true;
    ctx2d.drawImage(tmp, 0, 0, air.width, air.height);
  }
  function drawAir() {
    if (gl) {
      use(gl, P);
      gl.viewport(0, 0, air.width, air.height);
      gl.uniform2f(P.uRes, air.width, air.height);
      gl.uniform3fv(P.uLight, vec(lit.light));
      gl.uniform1f(P.uIntensity, lit.intensity);
      gl.uniform1f(P.uExposure, lit.exposure);
      gl.uniform1f(P.uFace, lit.face);
      gl.uniform3f(P.uMS, lit.ms[0], lit.ms[1], lit.ms[2]);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
    } else if (ctx2d) drawAirFlat();
  }

  /* A seeded star field: right ascension, declination, magnitude, colour. */
  var STARS = (function () {
    var s = 0x2f6b9a1d, out = [];
    function rnd() { s ^= s << 13; s ^= s >>> 17; s ^= s << 5; return (s >>> 0) / 4294967296; }
    for (var i = 0; i < 1500; i++) {
      var m = -1 + 7.2 * Math.pow(rnd(), 0.42);
      out.push([rnd() * 2 * Math.PI, Math.asin(rnd() * 2 - 1), m, rnd()]);
    }
    return out;
  })();
  function rgb(c, a) { return "rgba(" + Math.round(c[0]) + "," + Math.round(c[1]) + "," + Math.round(c[2]) + "," + a.toFixed(3) + ")"; }
  /* Layer 2: Sun, Moon and stars at full resolution. */
  function drawLights(ms) {
    var w = lightsCv.width, h = lightsCv.height, c = lightsCv.getContext("2d"), f = lit.face;
    var pxPerRad = h / VFOV;
    c.clearRect(0, 0, w, h);
    var night = lit.night, j = jd(ms), where = state.where;
    if (night > 0.02) {
      var lst = (gmst(j) + where.lon) * RAD, phi = where.lat * RAD, sp = Math.sin(phi), cp = Math.cos(phi);
      for (var i = 0; i < STARS.length; i++) {
        var st = STARS[i], H0 = lst - st[0], sd = Math.sin(st[1]), cd = Math.cos(st[1]);
        var alt = Math.asin(sp * sd + cp * cd * Math.cos(H0));
        if (alt < 0.01) continue;
        var az = Math.atan2(-Math.sin(H0), Math.tan(st[1]) * cp - sp * Math.cos(H0));
        var p = project([Math.cos(alt) * Math.sin(az), Math.sin(alt), Math.cos(alt) * Math.cos(az)], f, w, h);
        if (!p || p[0] < -2 || p[0] > w + 2 || p[1] < -2 || p[1] > h + 2) continue;
        var airmass = 1 / (Math.sin(alt) + 0.15 * Math.pow(alt / RAD + 3.885, -1.253));
        var mag = st[2] + 0.25 * airmass;
        var a = Math.min(1, Math.pow(2.512, 3.2 - mag)) * night;
        if (a < 0.03) continue;
        var tint = st[3] < 0.15 ? [255, 210, 170] : st[3] > 0.8 ? [190, 210, 255] : [255, 248, 240];
        var r = 0.6 + Math.max(0, 3.2 - mag) * 0.45;
        c.fillStyle = rgb(tint, a);
        c.beginPath(); c.arc(p[0], p[1], r, 0, 2 * Math.PI); c.fill();
        if (mag < 1.2) {
          var halo = c.createRadialGradient(p[0], p[1], 0, p[0], p[1], r * 5);
          halo.addColorStop(0, rgb(tint, a * 0.35)); halo.addColorStop(1, rgb(tint, 0));
          c.fillStyle = halo; c.beginPath(); c.arc(p[0], p[1], r * 5, 0, 2 * Math.PI); c.fill();
        }
      }
    }
    var sv = vec(lit.sun), mv = vec(lit.moon);
    /* The Moon: true phase, bright limb towards the Sun, drawn 2x its
       angular size so a TV shows it as more than a few pixels. */
    if (lit.moon.alt > -0.01) {
      var mp = project(mv, f, w, h);
      if (mp) {
        var rM = 0.0045 * 2 * pxPerRad, tm = transmit(mv, 0);
        var toward = project([mv[0] + (sv[0] - mv[0]) * 0.02, mv[1] + (sv[1] - mv[1]) * 0.02, mv[2] + (sv[2] - mv[2]) * 0.02], f, w, h);
        var ang = toward ? Math.atan2(toward[1] - mp[1], toward[0] - mp[0]) : 0;
        var bright = Math.min(1, 0.35 + 0.65 * Math.max(night, lit.useSun ? 0 : 1));
        var lc = [255 * Math.min(1, tm[0] * 1.15), 248 * Math.min(1, tm[1] * 1.1), 232 * tm[2]];
        var glowM = c.createRadialGradient(mp[0], mp[1], rM, mp[0], mp[1], rM * 7);
        glowM.addColorStop(0, rgb(lc, 0.22 * lit.moon.illum * bright)); glowM.addColorStop(1, rgb(lc, 0));
        c.fillStyle = glowM; c.beginPath(); c.arc(mp[0], mp[1], rM * 7, 0, 2 * Math.PI); c.fill();
        c.save();
        c.translate(mp[0], mp[1]); c.rotate(ang);
        c.fillStyle = "rgba(40,44,60," + (0.5 * bright).toFixed(3) + ")";
        c.beginPath(); c.arc(0, 0, rM, 0, 2 * Math.PI); c.fill();
        /* Lit part: the half facing the Sun plus or minus the terminator
           ellipse, whose half-width is r * |cos(phase angle)|. */
        var k = 2 * lit.moon.illum - 1;
        c.fillStyle = rgb(lc, 0.96 * bright);
        c.beginPath();
        c.arc(0, 0, rM, -Math.PI / 2, Math.PI / 2, false);
        c.ellipse(0, 0, Math.abs(k) * rM, rM, 0, Math.PI / 2, -Math.PI / 2, k > 0);
        c.fill();
        c.globalCompositeOperation = "source-atop";
        [[-0.25, -0.3, 0.32, 0.22], [0.2, -0.05, 0.25, 0.3], [-0.05, 0.35, 0.22, 0.16], [0.35, 0.4, 0.14, 0.12]].forEach(function (m) {
          c.fillStyle = "rgba(90,96,110," + (0.28 * bright).toFixed(3) + ")";
          c.beginPath(); c.ellipse(m[0] * rM, m[1] * rM, m[2] * rM, m[3] * rM, 0.4, 0, 2 * Math.PI); c.fill();
        });
        c.restore();
      }
    }
    /* The Sun: its disk coloured by the air in front of it, and glare. */
    if (lit.sun.alt > -0.02) {
      var sp2 = project(sv, f, w, h);
      if (sp2) {
        var ts = transmit(sv, 0), rS = 0.00465 * 2 * pxPerRad;
        var sc = [255, 255 * Math.min(1, ts[1] / Math.max(ts[0], 1e-6)), 255 * Math.min(1, ts[2] / Math.max(ts[0], 1e-6))];
        var glare = c.createRadialGradient(sp2[0], sp2[1], 0, sp2[0], sp2[1], rS * 16);
        glare.addColorStop(0, rgb(sc, 0.9)); glare.addColorStop(0.08, rgb(sc, 0.55)); glare.addColorStop(0.3, rgb(sc, 0.12)); glare.addColorStop(1, rgb(sc, 0));
        c.fillStyle = glare; c.beginPath(); c.arc(sp2[0], sp2[1], rS * 16, 0, 2 * Math.PI); c.fill();
        c.fillStyle = rgb([255, 252, 245], 1); c.beginPath(); c.arc(sp2[0], sp2[1], rS, 0, 2 * Math.PI); c.fill();
      }
    }
  }

  /* Layer 4: the hill line, at full resolution, in front of the clouds. */
  function drawHills() {
    var w = lightsCv.width, h = lightsCv.height, f = lit.face, l = vec(lit.light), pts = [], cols = 96;
    for (var i = 0; i <= cols; i++) {
      var qx = (i / cols * 2 - 1) * (w / h), lo = -1.6, hi = 0.6;
      for (var it = 0; it < 24; it++) {
        var mid = (lo + hi) / 2, v = view(qx, mid, f);
        if (Math.asin(v[1]) > hill(Math.atan2(v[0], v[2]))) hi = mid; else lo = mid;
      }
      pts.push([i / cols * w, (1 - hi) / 2 * h]);
    }
    var d = "M0," + h + " L" + pts.map(function (p) { return p[0].toFixed(1) + "," + p[1].toFixed(1); }).join(" L") + " L" + w + "," + h + " Z";
    var paths = hillsSvg.querySelectorAll("path");
    paths[0].setAttribute("d", d); paths[1].setAttribute("d", d);
    /* Airlight across the horizon, mixed with the lit land. */
    var tl = transmit(l, 0), up = Math.max(l[1] + 0.1, 0), stops = "";
    var amb = radiance([0, 1, 0], l, 6, 3);
    for (var s = 0; s <= 4; s++) {
      var qx2 = (s / 4 * 2 - 1) * (w / h), v2 = view(qx2, -0.62, f);
      v2 = [v2[0], 0.004, v2[2]];
      var L = radiance(v2, l, 6, 3), col = [];
      for (var k = 0; k < 3; k++) {
        var land = [0.045, 0.055, 0.04][k] * ((amb[k] * lit.intensity + lit.ms[k]) * 1.6 + 22 * lit.intensity * tl[k] * up * 0.088);
        col.push(display((land * 0.78 + (L[k] * lit.intensity + lit.ms[k] * 0.7) * 0.22) * lit.exposure));
      }
      stops += '<stop offset="' + (s / 4) + '" stop-color="rgb(' + col.join(",") + ')"/>';
    }
    var defs = hillsSvg.querySelectorAll("linearGradient");
    defs[0].innerHTML = stops;
    var deep = [0, 1, 2].map(function (k) { return display([0.045, 0.055, 0.04][k] * ((amb[k] * lit.intensity + lit.ms[k]) * 1.6) * lit.exposure * 0.6); });
    defs[1].innerHTML = '<stop offset="0.80" stop-color="rgb(' + deep.join(",") + ')" stop-opacity="0"/><stop offset="1" stop-color="rgb(' + deep.join(",") + ')" stop-opacity="0.85"/>';
  }

  /* Layer 3, every animation frame. */
  function drawClouds(ms) {
    if (!cgl || !lit) return;
    var w = Math.max(64, Math.round(lightsCv.width * scale)), h = Math.max(36, Math.round(lightsCv.height * scale));
    if (cloudsCv.width !== w || cloudsCv.height !== h) { cloudsCv.width = w; cloudsCv.height = h; }
    var l = vec(lit.light), tl = transmit(l, 2000), up = Math.max(l[1] + 0.1, 0), amb = radiance([0, 1, 0], l, 6, 3), e = lit.exposure;
    var t = reduced() ? 0 : (ms - startMs) / 1000 + (startMs / 1000) % 5000;
    use(cgl, C);
    cgl.viewport(0, 0, w, h);
    cgl.activeTexture(cgl.TEXTURE0);
    cgl.bindTexture(cgl.TEXTURE_2D, noiseTex);
    cgl.uniform1i(C.uNoise, 0);
    cgl.uniform2f(C.uRes, w, h);
    cgl.uniform1f(C.uTime, t);
    cgl.uniform1f(C.uCover, cloudCover(ms));
    cgl.uniform1f(C.uFace, lit.face);
    cgl.uniform3fv(C.uLight, l);
    cgl.uniform3f(C.uCloudLight, 22 * lit.intensity * tl[0] * up * 0.22 * e, 22 * lit.intensity * tl[1] * up * 0.22 * e, 22 * lit.intensity * tl[2] * up * 0.22 * e);
    cgl.uniform3f(C.uAmb, (amb[0] * lit.intensity + lit.ms[0]) * e, (amb[1] * lit.intensity + lit.ms[1]) * e, (amb[2] * lit.intensity + lit.ms[2]) * e);
    cgl.clearColor(0, 0, 0, 0);
    cgl.clear(cgl.COLOR_BUFFER_BIT);
    cgl.drawArrays(cgl.TRIANGLES, 0, 3);
    state.cloudFrames++;
  }

  /* Everything that changes slowly: the light, the atmosphere, the Sun,
     the Moon, the stars and the hills. */
  function drawSlow() {
    if (!node) return;
    var ms = now(), zone = currentZone();
    if (zone !== state.zone || !state.where) {
      state.zone = zone;
      state.where = locate(zone, new Date(ms).getTimezoneOffset());
    }
    lit = lighting(ms, state.where);
    state.lighting = { sunAlt: lit.sun.alt / RAD, sunAz: lit.sun.az / RAD, moonAlt: lit.moon.alt / RAD, illum: lit.moon.illum, light: lit.useSun ? "sun" : "moon", exposure: lit.exposure };
    drawAir();
    drawLights(ms);
    drawHills();
    drawClouds(ms);
    state.frames++;
  }
  /* The animation loop: clouds at up to 30 frames a second, at a
     resolution that keeps the device near 24. */
  function frame(ts) {
    raf = null;
    if (document.hidden || reduced() || !cgl) return;
    if (ts - lastFrame >= 33) {
      if (lastFrame) frameTimes.push(ts - lastFrame);
      lastFrame = ts;
      drawClouds(now());
      if (frameTimes.length >= 48) {
        var avg = frameTimes.reduce(function (a, b) { return a + b; }, 0) / frameTimes.length;
        state.fps = Math.round(1000 / avg);
        if (avg > 50 && scale > 0.3) scale = Math.max(0.3, scale * 0.85);
        else if (avg < 38 && scale < 0.8) scale = Math.min(0.8, scale * 1.08);
        state.scale = Math.round(scale * 100) / 100;
        frameTimes = [];
      }
    }
    raf = requestAnimationFrame(frame);
  }
  function animate() { if (!raf && cgl && !reduced() && !document.hidden) raf = requestAnimationFrame(frame); }
  function schedule() {
    if (slowTimer) clearTimeout(slowTimer);
    slowTimer = setTimeout(function () {
      if (!document.hidden) drawSlow();
      schedule();
    }, reduced() ? 60000 : 5000);
  }
  function boot() {
    air = document.querySelector("canvas.sky");
    if (!air) return;
    node = air.parentNode;
    lightsCv = node.querySelector("canvas.sky-lights");
    cloudsCv = node.querySelector("canvas.sky-clouds");
    hillsSvg = node.querySelector("svg.sky-hills");
    ASPECT = lightsCv.width / lightsCv.height;
    if (initGL()) state.mode = "webgl";
    else { ctx2d = air.getContext("2d"); state.mode = ctx2d ? "2d" : null; }
    state.scale = scale;
    drawSlow();
    schedule();
    animate();
    document.addEventListener("visibilitychange", function () { if (!document.hidden) { lastFrame = 0; drawSlow(); animate(); } });
  }

  root.GoyaSky = {
    contract: "arachnid.sky/0.1.0",
    locate: locate, sun: sun, moon: moon, radiance: radiance, lighting: lighting, transmit: transmit, project: project, view: view, hill: hill,
    state: function () { var s = JSON.parse(JSON.stringify(state)); s.clouds = !!cgl; return s; },
    /* Test hook: draw the sky for a fixed instant (null returns to the clock). */
    pin: function (ms) { state.pinned = ms === null ? null : Number(ms); state.where = null; drawSlow(); }
  };
  if (typeof document !== "undefined") {
    if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", boot);
    else boot();
  }
})(typeof window !== "undefined" ? window : this);
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_table_is_valid_and_resolves_legacy_names() {
        let z = zones_js().unwrap();
        assert!(z.contains("\"Europe/London\":[51.51,-0.13]"), "{}", &z[..200]);
        assert!(z.contains("\"Asia/Calcutta\":[22.53,88.37]"));
        assert!(z.contains("\"GB\":[51.51,-0.13]"));
        assert!(!z.contains("\"UTC\""), "UTC has no place; the offset decides");
        assert!(z.matches(":[").count() > 500);
    }

    #[test]
    fn script_is_complete_and_deterministic() {
        let s = script();
        assert!(!s.contains("/*ZONES*/"));
        assert!(s.contains(CONTRACT) && CONTRACT == "arachnid.sky/0.1.0");
        assert_eq!(script(), s);
    }

    #[test]
    fn layers_follow_the_node() {
        let m = canvas_markup(1920, 1080);
        assert!(m.starts_with(r#"<canvas class="sky" width="320" height="180""#), "{m}");
        assert!(m.contains(r#"<canvas class="sky-lights" width="1920" height="1080""#));
        assert!(m.contains(r#"<canvas class="sky-clouds" width="1920" height="1080""#));
        assert!(m.contains(r#"<svg class="sky-hills" viewBox="0 0 1920 1080""#));
        assert!(canvas_markup(800, 800).starts_with(r#"<canvas class="sky" width="180" height="180""#));
    }
}
