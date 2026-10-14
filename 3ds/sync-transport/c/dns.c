// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

#include "dns.h"
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <ctype.h>
#include <errno.h>
#ifndef DNS_PARSER_TEST
#include <3ds.h>
#include <arpa/inet.h>
#include <sys/socket.h>
#include <sys/select.h>
#include <fcntl.h>
#include <unistd.h>
#endif

static unsigned word(const uint8_t *p) { return (unsigned)p[0] * 256 + p[1]; }
/* Follow compression only within this packet; a jump budget rejects cycles. */
static int name(const uint8_t *p, size_t n, size_t *offset, char out[254]) {
    size_t at = *offset, end = 0, used = 0;
    unsigned steps = 0;
    while (at < n && steps++ < 128) {
        unsigned len = p[at++];
        if (!len) {
            *offset = end ? end : at;
            out[used] = 0;
            return 0;
        }
        if ((len & 0xc0) == 0xc0) {
            if (at >= n) return -1;
            if (!end) end = at + 1;
            at = ((len & 63) << 8) | p[at];
            continue;
        }
        if (len > 63 || at + len > n || used + len + (used != 0) > 253) return -1;
        if (used) out[used++] = '.';
        for (unsigned i = 0; i < len; i++) {
            unsigned ch = p[at++];
            if (!ch || ch == '.') return -1;
            out[used++] = (char)tolower(ch);
        }
    }
    return -1;
}

static int parse(const uint8_t *p, size_t n, unsigned id, const char *host,
                 char ip[16], char error[160]) {
    if (n < 12 || word(p) != id) return 1; /* unrelated datagram */
    unsigned flags = word(p + 2);
    if (!(flags & 0x8000) || (flags & 0x7800)) goto malformed;
    if (flags & 0x0200) { snprintf(error, 160, "DNS truncated; TCP required (unsupported)"); return -1; }
    if (flags & 15) { snprintf(error, 160, "DNS RCODE=%u", flags & 15); return -1; }
    if (word(p + 4) != 1) goto malformed;
    size_t offset = 12;
    char question[254];
    if (name(p, n, &offset, question) || strcmp(question, host) || offset + 4 > n ||
        word(p + offset) != 1 || word(p + offset + 2) != 1) goto malformed;
    offset += 4;
    size_t answer_start = offset;
    unsigned answers = word(p + 6);
    if (answers > 64) goto malformed;
    char wanted[254];
    strcpy(wanted, host);
    /* Rescan bounded answer list for each CNAME, independent of record order. */
    for (unsigned chain = 0; chain < 16; chain++) {
        offset = answer_start;
        bool followed = false;
        for (unsigned i = 0; i < answers; i++) {
            char owner[254];
            if (name(p, n, &offset, owner) || offset + 10 > n) goto malformed;
            unsigned type = word(p + offset), cls = word(p + offset + 2);
            unsigned len = word(p + offset + 8);
            offset += 10;
            if (offset + len > n) goto malformed;
            if (cls == 1 && !strcmp(owner, wanted)) {
                if (type == 1 && len == 4) {
                    snprintf(ip, 16, "%u.%u.%u.%u", p[offset], p[offset+1], p[offset+2], p[offset+3]);
                    return 0;
                }
                if (type == 5) {
                    size_t cname_offset = offset;
                    char target[254];
                    if (name(p, n, &cname_offset, target) || cname_offset != offset + len) goto malformed;
                    strcpy(wanted, target);
                    followed = true;
                    break;
                }
            }
            offset += len;
        }
        if (!followed) { snprintf(error, 160, "DNS response contains no matching IPv4 address"); return -1; }
    }
    snprintf(error, 160, "DNS CNAME chain too long or cyclic");
    return -1;
malformed:
    snprintf(error, 160, "Malformed DNS response");
    return -1;
}

#ifndef DNS_PARSER_TEST
int probe_dns_with_context(const char *host, char ip[16], char error[160],
                           bool (*cancelled)(void *), void *context, bool allow_override) {
    struct in_addr literal;
    if (inet_pton(AF_INET, host, &literal) == 1) {
        snprintf(ip, 16, "%s", host);
        error[0] = 0;
        return 0;
    }
    uint8_t query[512] = {0}, response[512];
    char normalized[254];
    size_t length = strlen(host), qlen = 12;
    ip[0] = error[0] = 0;
    if (!length || length > 253) { snprintf(error, 160, "Invalid DNS hostname length"); return -1; }
    for (size_t i = 0; i <= length; i++) normalized[i] = (char)tolower((unsigned char)host[i]);
    uint16_t random_id;
    Result random_result = PS_GenerateRandomBytes(&random_id, sizeof(random_id));
    if (R_FAILED(random_result)) {
        snprintf(error, 160, "DNS secure random failed: result=%08lx", (unsigned long)random_result);
        return -1;
    }
    unsigned id = random_id;
    query[0] = id >> 8; query[1] = id; query[2] = 1; query[5] = 1;
    const char *label = normalized;
    while (*label) {
        const char *dot = strchr(label, '.');
        size_t size = dot ? (size_t)(dot - label) : strlen(label);
        if (!size || size > 63) { snprintf(error, 160, "Invalid DNS hostname label"); return -1; }
        query[qlen++] = size;
        memcpy(query + qlen, label, size); qlen += size;
        if (!dot) break;
        label = dot + 1;
        if (!*label) { snprintf(error, 160, "Trailing DNS dot unsupported"); return -1; }
    }
    query[qlen++] = 0; query[qlen++] = 0; query[qlen++] = 1;
    query[qlen++] = 0; query[qlen++] = 1;
    SOCU_DNSTableEntry servers[16] = {0};
    unsigned count = 0;
    const char *override_path = "sdmc:/3ds/anki-https-probe/dns-server.txt";
#ifndef ANKI_3DS_EMULATOR
    allow_override = false;
#endif
    FILE *override = allow_override ? fopen(override_path, "rb") : NULL;
    if (override) {
        char address[32] = {0};
        size_t bytes = fread(address, 1, sizeof(address) - 1, override);
        bool read_error = ferror(override) != 0;
        int extra = fgetc(override);
        fclose(override);
        /* Exactly one IPv4 address, optionally followed by LF or CRLF. */
        if (bytes && address[bytes - 1] == '\n') {
            address[--bytes] = 0;
            if (bytes && address[bytes - 1] == '\r') address[--bytes] = 0;
        }
        if (read_error || extra != EOF || memchr(address, 0, bytes) ||
            inet_pton(AF_INET, address, &servers[0].ip) != 1 || !servers[0].ip.s_addr) {
            snprintf(error, 160, "Invalid dns-server.txt: expected one IPv4 line");
            return -1;
        }
        servers[0].family = AF_INET;
        count = 1;
    } else {
        if (allow_override && errno != ENOENT) {
            snprintf(error, 160, "Cannot read dns-server.txt: errno=%d", errno);
            return -1;
        }
        // Horizon's DNS-table option requires the documented 336-byte buffer.
        // Sixteen public 20-byte entries provided only 320 bytes and physical
        // hardware returned EINVAL before any DNS or TLS operation.
        union { uint32_t alignment; uint8_t bytes[336]; } table = {0};
        socklen_t size = sizeof(table.bytes);
        int result = SOCU_GetNetworkOpt(SOL_CONFIG, NETOPT_DNS_TABLE, table.bytes, &size);
        if (result) {
            snprintf(error, 160, "DNS configuration failed: result=%d errno=%d requested=336 returned=%u", result, errno, (unsigned)size);
            return -1;
        }
        if (size > sizeof(table.bytes)) {
            snprintf(error, 160, "DNS configuration returned oversized table: %u", (unsigned)size);
            return -1;
        }
        count = size / sizeof(servers[0]);
        if (count > 16) count = 16;
        memcpy(servers, table.bytes, count * sizeof(servers[0]));
    }
    bool configured = false;
    /* Horizon exposes two configured DNS servers. Ignore unused table entries. */
    if (count > 2) count = 2;
    for (unsigned i = 0; i < count; i++) {
        if (servers[i].family != AF_INET || !servers[i].ip.s_addr) continue;
        configured = true;
        int fd = socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP);
        if (fd < 0) { snprintf(error, 160, "DNS socket failed: errno=%d", errno); continue; }
        if (fcntl(fd, F_SETFL, O_NONBLOCK) < 0) {
            snprintf(error, 160, "DNS nonblocking mode failed: errno=%d", errno); close(fd); continue;
        }
        struct sockaddr_in address = {0};
        address.sin_family = AF_INET; address.sin_port = htons(53); address.sin_addr = servers[i].ip;
        uint64_t deadline = osGetTime() + 5000;
        if (connect(fd, (struct sockaddr *)&address, sizeof(address)) < 0 ||
            send(fd, query, qlen, 0) != (ssize_t)qlen) {
            snprintf(error, 160, "DNS send failed: errno=%d", errno); close(fd); continue;
        }
        snprintf(error, 160, "DNS timeout (5000 ms/server)");
        while (osGetTime() < deadline) {
            if (cancelled && cancelled(context)) { snprintf(error, 160, "DNS cancelled"); close(fd); return -1; }
            fd_set read_set; FD_ZERO(&read_set); FD_SET(fd, &read_set);
            uint64_t now = osGetTime();
            if (now >= deadline) break;
            uint64_t remaining = deadline - now;
            unsigned ms = remaining < 100 ? remaining : 100;
            struct timeval timeout = {0, ms * 1000};
            int ready = select(fd + 1, &read_set, NULL, NULL, &timeout);
            if (ready < 0) { snprintf(error, 160, "DNS select failed: errno=%d", errno); break; }
            if (!ready) continue;
            ssize_t bytes = recv(fd, response, sizeof(response), 0);
            if (bytes < 0) {
                if (errno == EAGAIN || errno == EWOULDBLOCK) continue;
                snprintf(error, 160, "DNS receive failed: errno=%d", errno); break;
            }
            int parsed = parse(response, bytes, id, normalized, ip, error);
            if (!parsed) { close(fd); error[0] = 0; return 0; }
            if (parsed < 0) break;
        }
        close(fd);
    }
    if (!configured) snprintf(error, 160, "No configured IPv4 DNS server: entries=%u families=%lu,%lu",
                              count, (unsigned long)servers[0].family, (unsigned long)servers[1].family);
    return -1;
}
#else
#include <assert.h>
int main(void) {
    uint8_t packet[] = {0x12,0x34,0x81,0x80,0,1,0,1,0,0,0,0,
        1,'a',0,0,1,0,1,0xc0,0x0c,0,1,0,1,0,0,0,1,0,4,1,2,3,4};
    char ip[16], error[160];
    assert(parse(packet, sizeof(packet), 0x1234, "a", ip, error) == 0);
    assert(!strcmp(ip, "1.2.3.4"));
    assert(parse(packet, sizeof(packet), 0x9999, "a", ip, error) == 1);
    assert(parse(packet, sizeof(packet)-1, 0x1234, "a", ip, error) == -1);
    assert(parse(packet, sizeof(packet), 0x1234, "b", ip, error) == -1);
    uint8_t alias[] = {0x12,0x34,0x81,0x80,0,1,0,2,0,0,0,0,
        1,'a',0,0,1,0,1,
        0xc0,0x0c,0,5,0,1,0,0,0,1,0,3,1,'b',0,
        1,'b',0,0,1,0,1,0,0,0,1,0,4,5,6,7,8};
    assert(parse(alias, sizeof(alias), 0x1234, "a", ip, error) == 0);
    assert(!strcmp(ip, "5.6.7.8"));
    alias[32] = 'a';
    assert(parse(alias, sizeof(alias), 0x1234, "a", ip, error) == -1);
    packet[2] |= 2;
    assert(parse(packet, sizeof(packet), 0x1234, "a", ip, error) == -1);
    packet[2] &= ~2;
    packet[12] = 0xc0; packet[13] = 12;
    assert(parse(packet, sizeof(packet), 0x1234, "a", ip, error) == -1);
    puts("DNS parser tests passed");
}
#endif
struct LegacyCancel { bool (*function)(void); };
static bool legacy_cancel(void *context) {
    struct LegacyCancel *legacy = context;
    return legacy->function && legacy->function();
}
int probe_dns(const char *host, char ip[16], char error[160], bool (*cancelled)(void)) {
    struct LegacyCancel legacy = {cancelled};
    return probe_dns_with_context(host, ip, error, legacy_cancel, &legacy, true);
}
