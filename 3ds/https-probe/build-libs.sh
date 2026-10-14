#!/bin/sh
# Build isolated current TLS/HTTP libraries without changing devkitPro packages.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
SOURCES="$ROOT/out/3ds/https-sources"
PREFIX="$ROOT/out/3ds/https-prefix"
BUILD="$ROOT/out/3ds/https-libs-build"
DKP=${DEVKITPRO:-/opt/devkitpro}
export PATH="$DKP/devkitARM/bin:$PATH"
mkdir -p "$SOURCES" "$PREFIX" "$BUILD"
fetch() {
    name=$1; url=$2; hash=$3
    [ -f "$SOURCES/$name" ] || curl --fail --location --proto '=https' --tlsv1.2 "$url" -o "$SOURCES/$name"
    printf '%s  %s\n' "$hash" "$SOURCES/$name" | shasum -a 256 -c -
}
fetch mbedtls.tar.bz2 https://github.com/Mbed-TLS/mbedtls/releases/download/mbedtls-3.6.7/mbedtls-3.6.7.tar.bz2 a7e8bcbec0e6f761b4af24f25677626b35f762f68eef79c08677a363212d11f6
fetch curl.tar.xz https://curl.se/download/curl-8.22.0.tar.xz f7ef3ae8a22e521f289803fe93543eb64c329b58aa73a9e224dfd915a2a5f4f7
[ -d "$SOURCES/mbedtls-3.6.7" ] || tar -xjf "$SOURCES/mbedtls.tar.bz2" -C "$SOURCES"
[ -d "$SOURCES/curl-8.22.0" ] || tar -xJf "$SOURCES/curl.tar.xz" -C "$SOURCES"
python3 - "$SOURCES" <<'PY'
import pathlib,re,sys
s=pathlib.Path(sys.argv[1])
p=s/'mbedtls-3.6.7/include/mbedtls/mbedtls_config.h'
t=p.read_text()
for name in ('MBEDTLS_NET_C','MBEDTLS_TIMING_C'):
    t=re.sub(r'^/*#define '+name+r'$','//#define '+name,t,flags=re.M)
for name in ('MBEDTLS_ENTROPY_HARDWARE_ALT','MBEDTLS_NO_PLATFORM_ENTROPY','MBEDTLS_PLATFORM_MS_TIME_ALT'):
    t=re.sub(r'^/*#define '+name+r'$','#define '+name,t,flags=re.M)
p.write_text(t)
p=s/'curl-8.22.0/lib/curl_addrinfo.c';t=p.read_text()
old='    if((size_t)ai->ai_addrlen < ss_size)\n      continue;'
new='    /* SOC returns 8 for IPv4; its sockaddr_in storage is still complete. */\n    if((size_t)ai->ai_addrlen < ss_size\n#ifdef __3DS__\n       && !(ai->ai_family == AF_INET && ai->ai_addrlen == 8)\n#endif\n      )\n      continue;'
if old in t:t=t.replace(old,new)
else:assert new in t
p.write_text(t)
p=s/'curl-8.22.0/lib/cf-socket.c';t=p.read_text()
start=t.index('static void tcpkeepalive(');end=t.index('\n/**',start)
part=t[start:end]
if '#ifdef SO_KEEPALIVE' not in part:
    part=part.replace('  int optval =', '#ifdef SO_KEEPALIVE\n  int optval =',1)
    at=part.rfind('\n}')
    part=part[:at]+'\n#else\n  (void)cf; (void)data; (void)sockfd;\n#endif'+part[at:]
    t=t[:start]+part+t[end:]
p.write_text(t)
old='#if defined(HAVE_FCNTL) && !defined(SOCK_CLOEXEC)'
new='#if defined(HAVE_FCNTL) && !defined(SOCK_CLOEXEC) && !defined(__3DS__)'
if old in t: t=t.replace(old,new)
else: assert new in t
p.write_text(t)
PY
FLAGS="-march=armv6k -mtune=mpcore -mfloat-abi=hard -mfpu=vfp -mtp=soft -D__3DS__ -I$DKP/libctru/include -ffunction-sections -fdata-sections -O2"
cat > "$BUILD/toolchain.cmake" <<TC
set(CMAKE_SYSTEM_NAME Generic)
set(CMAKE_SYSTEM_PROCESSOR armv6k)
set(CMAKE_C_COMPILER $DKP/devkitARM/bin/arm-none-eabi-gcc)
set(CMAKE_CXX_COMPILER $DKP/devkitARM/bin/arm-none-eabi-g++)
set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)
set(CMAKE_C_FLAGS_INIT "$FLAGS")
TC
cmake -S "$SOURCES/mbedtls-3.6.7" -B "$BUILD/mbedtls" -DCMAKE_TOOLCHAIN_FILE="$BUILD/toolchain.cmake" -DCMAKE_INSTALL_PREFIX="$PREFIX" -DENABLE_TESTING=OFF -DENABLE_PROGRAMS=OFF -DUSE_SHARED_MBEDTLS_LIBRARY=OFF -DUSE_STATIC_MBEDTLS_LIBRARY=ON
cmake --build "$BUILD/mbedtls" -j4
cmake --install "$BUILD/mbedtls"
arm-none-eabi-gcc $FLAGS -I"$PREFIX/include" -c "$ROOT/3ds/sync-transport/c/entropy.c" -o "$PREFIX/lib/entropy-3ds.o"
mkdir -p "$BUILD/curl"
cd "$BUILD/curl"
ac_cv_header_pthread_h=no CC=arm-none-eabi-gcc AR=arm-none-eabi-ar RANLIB=arm-none-eabi-ranlib CFLAGS="$FLAGS" CPPFLAGS="-D__3DS__ -I$DKP/libctru/include -I$PREFIX/include" LDFLAGS="-L$PREFIX/lib -L$DKP/libctru/lib -specs=3dsx.specs" LIBS="-lctru" "$SOURCES/curl-8.22.0/configure" --host=arm-none-eabi --prefix="$PREFIX" --disable-shared --enable-static --with-mbedtls="$PREFIX" --without-openssl --without-zlib --without-brotli --without-zstd --without-libpsl --without-libidn2 --without-libssh2 --without-librtmp --disable-ipv6 --disable-unix-sockets --disable-threaded-resolver --disable-socketpair --disable-manual --disable-ipfs --disable-alt-svc --disable-hsts --disable-proxy --disable-ftp --disable-file --disable-ldap --disable-ldaps --disable-rtsp --disable-dict --disable-telnet --disable-tftp --disable-pop3 --disable-imap --disable-smtp --disable-gopher --disable-mqtt --disable-smb --disable-websockets --disable-ntlm --disable-negotiate --without-ca-bundle --without-ca-path
make -C lib -j4
make -C lib install
make -C include install
