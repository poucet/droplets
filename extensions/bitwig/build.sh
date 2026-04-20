#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

# Use Homebrew's openjdk for the `jar` tool (macOS ships no JDK by default).
JAVA_HOME="${JAVA_HOME:-/opt/homebrew/opt/openjdk/libexec/openjdk.jdk/Contents/Home}"
export JAVA_HOME
export PATH="$JAVA_HOME/bin:$PATH"

BITWIG_JAR="libs/bitwig.jar"
BITWIG_SRC="/Applications/Bitwig Studio.app/Contents/Java/bitwig.jar"
OUT="build/Droplets.bwextension"

if [ ! -f "$BITWIG_JAR" ]; then
    echo "→ copying Bitwig API jar from $BITWIG_SRC"
    cp "$BITWIG_SRC" "$BITWIG_JAR"
fi

rm -rf "$OUT" build
mkdir -p build

echo "→ compiling Kotlin"
# kotlinc only recognizes `.jar` as a jar output; we rename afterwards.
kotlinc -cp "$BITWIG_JAR" -jvm-target 21 -include-runtime -d build/Droplets.jar \
    src/main/kotlin/com/simply/droplets/*.kt

echo "→ adding META-INF resources"
jar uf build/Droplets.jar -C src/main/resources META-INF
mv build/Droplets.jar "$OUT"

echo "✓ built $OUT ($(du -h "$OUT" | cut -f1))"
