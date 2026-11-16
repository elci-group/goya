#!/bin/sh

# Report whether basic local Android compilation prerequisites are present.
# This is an inspection-only command: it does not install SDK components,
# resolve Gradle/AGP dependencies, build the project, or start an emulator.
set -u

status=PRESENT
say() { printf '%s\n' "$*"; }

say 'Aragon Android build preflight'
say 'Build target: platform/android/gradle (compileSdk 35)'

java_cmd=
javac_cmd=
if [ -n "${JAVA_HOME-}" ]; then
    if [ -x "$JAVA_HOME/bin/java" ]; then
        java_cmd=$JAVA_HOME/bin/java
    else
        say 'JDK: MISSING (JAVA_HOME does not contain bin/java)'
    fi
else
    java_cmd=$(command -v java 2>/dev/null || true)
fi
if [ -n "${JAVA_HOME-}" ]; then
    if [ -x "$JAVA_HOME/bin/javac" ]; then
        javac_cmd=$JAVA_HOME/bin/javac
    else
        say 'JDK compiler: MISSING (JAVA_HOME does not contain bin/javac)'
    fi
else
    javac_cmd=$(command -v javac 2>/dev/null || true)
fi

if [ -z "$java_cmd" ]; then
    say 'JDK runtime: MISSING (java not found; JDK 17 or newer is required)'
    status=MISSING
fi
if [ -z "$javac_cmd" ]; then
    say 'JDK compiler: MISSING (javac not found; a JDK is required)'
    status=MISSING
fi

if [ -n "$java_cmd" ]; then
    version_output=$("$java_cmd" -version 2>&1 || true)
    version=$(printf '%s\n' "$version_output" | sed -n '1s/.*version "\([^"]*\)".*/\1/p')
    major=$(printf '%s\n' "$version" | sed 's/^1\.//' | cut -d. -f1)
    case $major in
        ''|*[!0-9]*)
            say 'JDK runtime: UNVERIFIED (could not determine Java version)'
            status=MISSING
            ;;
        *)
            if [ "$major" -ge 17 ]; then
                say "JDK runtime: PRESENT (Java $version)"
            else
                say "JDK runtime: MISSING (Java $version; JDK 17 or newer is required)"
                status=MISSING
            fi
            ;;
    esac
fi
if [ -n "$javac_cmd" ]; then
    compiler_output=$("$javac_cmd" -version 2>&1 || true)
    compiler_version=$(printf '%s\n' "$compiler_output" | sed -n '1s/.*javac \([^ ]*\).*/\1/p')
    compiler_major=$(printf '%s\n' "$compiler_version" | sed 's/^1\.//' | cut -d. -f1)
    case $compiler_major in
        ''|*[!0-9]*)
            say 'JDK compiler: UNVERIFIED (could not determine javac version)'
            status=MISSING
            ;;
        *)
            if [ "$compiler_major" -ge 17 ]; then
                say "JDK compiler: PRESENT (javac $compiler_version)"
            else
                say "JDK compiler: MISSING (javac $compiler_version; JDK 17 or newer is required)"
                status=MISSING
            fi
            ;;
    esac
fi

# Empty ANDROID_SDK_ROOT is common in shell profiles; allow ANDROID_HOME to
# provide the fallback in that case as well as when SDK_ROOT is unset.
sdk_root=${ANDROID_SDK_ROOT-}
if [ -z "$sdk_root" ]; then
    sdk_root=${ANDROID_HOME-}
fi
if [ -z "$sdk_root" ]; then
    for candidate in "$HOME/Android/Sdk" /usr/lib/android-sdk /opt/android-sdk; do
        if [ -d "$candidate" ]; then
            sdk_root=$candidate
            break
        fi
    done
fi

if [ -z "$sdk_root" ] || [ ! -d "$sdk_root" ]; then
    say 'Android SDK: MISSING (set ANDROID_SDK_ROOT or ANDROID_HOME to an installed SDK)'
    status=MISSING
else
    say "Android SDK root: $sdk_root"
    if [ -f "$sdk_root/platforms/android-35/android.jar" ]; then
        say 'Android SDK Platform 35 android.jar: PRESENT'
    else
        say 'Android SDK Platform 35 android.jar: MISSING (install platform android-35)'
        status=MISSING
    fi
fi

if command -v adb >/dev/null 2>&1; then
    device_lines=$(adb devices 2>/dev/null | sed '1d' | sed '/^[[:space:]]*$/d')
    if [ -n "$device_lines" ]; then
        say 'Android device/emulator: detected (not required for compile or JVM checks)'
    else
        say 'Android device/emulator: none detected (not required for compile or JVM checks)'
    fi
else
    say 'Android device/emulator: adb unavailable (device qualification not performed)'
fi

say 'Gradle/AGP resolution, Build Tools availability, and compilation: NOT CHECKED'
say "Checked prerequisites: $status"
[ "$status" = PRESENT ]
