#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
source 3ds/toolchain.sh
prefix="$PWD/out/3ds/https-prefix"
build="$PWD/out/3ds/https-probe"
mkdir -p "$build" "$PWD/out/3ds/https-sources"
ca="$PWD/out/3ds/https-sources/cacert.pem"
if [[ ! -f "$ca" ]]; then
    curl --fail --location --proto '=https' --tlsv1.2 https://curl.se/ca/cacert.pem -o "$ca"
fi
printf '%s  %s\n' a41b5d356aea97a529fe27e0f7316d2f9d946d75927476cf9cf1b90637d00505 "$ca" | shasum -a 256 -c -
git_hash="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
flags=(-march=armv6k -mtune=mpcore -mfloat-abi=hard -mfpu=vfp -mtp=soft -D__3DS__ -DANKI_3DS_EMULATOR -DGIT_HASH="\"$git_hash\"" -O2 -g -Wall -Wextra -Werror -ffunction-sections -fdata-sections -I"$DEVKITPRO/libctru/include" -I"$prefix/include" -I"$PWD/3ds/sync-transport/c")
for source in main; do
    arm-none-eabi-gcc "${flags[@]}" -c "3ds/https-probe/$source.c" -o "$build/$source.o"
done
arm-none-eabi-gcc "${flags[@]}" -c 3ds/sync-transport/c/dns.c -o "$build/dns.o"
arm-none-eabi-gcc "${flags[@]}" -specs="$PWD/3ds/old3ds.specs" -Wl,--gc-sections -Wl,-Map,"$build/https-probe.map" \
    "$build/main.o" "$build/dns.o" "$prefix/lib/entropy-3ds.o" \
    -L"$prefix/lib" -L"$DEVKITPRO/libctru/lib" \
    -lcurl -lmbedtls -lmbedx509 -lmbedcrypto -lctru -lm -o "$build/anki-https-probe.elf"
arm-none-eabi-readelf -h -A "$build/anki-https-probe.elf" > "$build/elf-attributes.txt"
grep -q 'Tag_CPU_arch: v6K' "$build/elf-attributes.txt"
grep -q 'Tag_FP_arch: VFPv2' "$build/elf-attributes.txt"
smdhtool --create "Anki HTTPS probe v3" "Direct secure Wi-Fi transport test v3" "Anki 3DS contributors" "$DEVKITPRO/libctru/default_icon.png" "$build/anki-https-probe.smdh"
3dsxtool "$build/anki-https-probe.elf" "$build/anki-https-probe.3dsx" --smdh="$build/anki-https-probe.smdh"
shasum -a 256 "$build/anki-https-probe.3dsx" > "$build/sha256.txt"
cc -Wall -Wextra -Werror -DDNS_PARSER_TEST 3ds/sync-transport/c/dns.c -o "$build/dns-parser-test"
"$build/dns-parser-test"
