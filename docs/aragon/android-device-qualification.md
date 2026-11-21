# Aragon device qualification (Phase 4)

Recorded 18 November 2026. `scripts/check-android-device.py` builds the
device fixtures in `crates/goya-sdk/tests/android-device/` with the locked
toolchain ([`android-toolchain-lock.md`](android-toolchain-lock.md)), signs
the release APKs with a throwaway test key, installs them on the one attached
device or emulator over adb, and checks the running app against the Rust
reference. It restores the display size, density and rotation afterwards.

```sh
JAVA_HOME=~/.local/opt/jdk-21.0.13+9 ANDROID_HOME=~/.local/opt/android-sdk \
    python3 scripts/check-android-device.py
```

Exit status: 0 when every check passes, 1 when a check fails, 2 when the
environment or device is missing. Before querying the device, the script also
checks that `ANDROID_HOME/platform-tools/adb`, the locked 34.0.0 `zipalign`
and `apksigner`, and `JAVA_HOME/bin/java` and `keytool` exist and can run; a
missing tool is an environment error (exit 2).

Pass `--report /path/to/aragon-device.json` to capture machine-readable evidence
from the same run. The versioned report includes the device API level and
fingerprint, fixture source and unsigned release APK SHA-256 hashes, and each
check result. Each geometry case records its viewport and density, requested
scroll limits, Rust reference rows, matched Android nodes, and the observed
accessibility tree's classes and bounds. A failed or interrupted qualification
can leave partial evidence. Use `exit_code` to determine whether qualification
passed.

## What is checked

**Geometry (ARG-G05).** The observer is the platform's accessibility tree
(`uiautomator dump`), not the app. For every element, the on-screen bounds of
its view, of its content (`TextView`, `Button`, `EditText`) and of its scroll
layer must equal the bounds computed by
`crates/goya-sdk/examples/aragon_device_expect.rs` from `goya_lang::layout`,
`Frame::scroll_limits` and the pixel rule in
[`android-scene.md`](android-scene.md). Elements that are fully clipped
must be absent. The layout fixture has proportional grids, an element clamped
into its parent, a negative position, nested cells inside a scrolled group,
and horizontal and vertical scroll viewports. It is checked:

* at four display configurations: the physical 1080×2340 at densities 440
  (2.75) and 420 (2.625), 900×1600 at 380 (2.375), and 1200×1920 at 480 (3.0);
* in each configuration, with each scroll viewport dragged to its limit and
  back to 0;
* in landscape, after the activity is recreated with a new viewport.

The viewport must lie clear of the status bar, navigation bar and display
cutout.

**Behaviour (ARG-G05, ARG-G07).** GSSH/2 cannot show state on screen, so the
oracle is the D1 overflow failure. The app shows `Action failed:
ARITHMETIC_OVERFLOW` in a toast, read from `dumpsys notification`. With the
counter fixture:

1. Adding `9223372036854775807` succeeds; adding it again overflows, so the
   total was stored.
2. Each of `01`, `-0`, `+1`, ` 1`, `1.0` and `abc` is refused before dispatch
   (no overflow), and a following `1` still overflows. Refused fields
   therefore changed nothing, as `convert_field` requires.
3. `-1`, then `1` succeed and a second `1` overflows: the D1 arithmetic is
   exact at the limit.
4. **Recreation.** After rotating the device, the field text is restored and
   `1` still overflows, so session state survived.
5. **Process death.** After going home and killing the process, the field
   text is restored from saved state, `1` succeeds, and the maximum value then
   overflows. State was therefore reset to its initial value, 0, as the
   `session` lifetime requires.

**Input and accessibility (ARG-G08).** Tapping a field opens the soft keyboard
(`mInputShown=true`). Buttons are clickable and expose their label as text,
and scroll viewports are reported as `android.widget.ScrollView` with
`scrollable=true` and scroll actions. Fields are focusable but have **no
accessible name** (`NAF=true`), because GSSH/2 gives an `@input` no label.
This is recorded as a gap, not hidden. Text is typed with `adb shell input
text`, which sends key events; the keyboard's composition path was not
exercised.

## Fixes found by running on Android

* **Edge-to-edge and action bar.** On Android 15, the default theme's action
  bar and the enforced edge-to-edge layout covered the scene. Scene projects
  now use `Theme.Material.Light.NoActionBar`, and `GoyaSceneView` treats
  system-bar and cutout insets as the viewport edge (`WindowInsets` on API 30
  and later, `systemWindowInsets` before that).
* **Scroll accessibility.** Scroll layers now report `canScrollVertically` and
  `canScrollHorizontally`, the `ScrollView` role, and forward and backward
  scroll actions of one page.
* **Harness fixes, not app defects.** The accessibility tree orders siblings
  by position, so the harness matches nodes by bounds. Drags are per axis.

## Results

| Device | Android | Display | Result |
| --- | --- | --- | --- |
| Emulator 37.2.13, `system-images;android-35;default;x86_64` r2, Pixel 5 profile | 15 (API 35), `AE3A.240806.019` | 1080×2340 @ 440 | 72 of 72 checks passed |
| Emulator 37.2.13, `system-images;android-23;default;x86_64` r10, Pixel 5 profile | 6.0 (API 23) | 1080×2340 @ 440 | 72 of 72 checks passed; toast text is not recorded before Android 11, so the oracle is a toast from the app's package (it shows toasts only for D1 failures) |

Fixture APKs (unsigned release, before test signing), built by this commit:
`layout` `40f89dd7…7a976`, `counter` `c792e345…73979`. The counter hash is also
the `scene` hash in `scripts/check-android-apk-build.sh`.

Logs: [`evidence/phase4-device-api35.log`](evidence/phase4-device-api35.log),
[`evidence/phase4-device-api23.log`](evidence/phase4-device-api23.log).

On Android 6.0 the code path for insets before API 30 is the one exercised, and
process death used `am kill` in the final run. The script falls back to
`kill -9` as root (what the low-memory killer does) only if `am kill` leaves
the process alive, and says so in its output.

## Not established

* **Physical device (ARG-G06).** None was attached; all results are from
  emulators.
* **TalkBack.** AOSP images do not include it. Only the accessibility tree
  that TalkBack reads was checked; no one has used the app with a screen
  reader.
* **IME composition**, hardware keyboards, and right-to-left locales.
* **Durable state** (still refused) and the ordering of durable writes on a
  crash.
* **Scroll offsets across recreation.** They are not preserved and reset to 0.
* **Pixel rendering** (colours, text placement). Bounds are checked, not
  pixels.
* **Release signing.** The APKs were signed with a throwaway key for
  installation only.
