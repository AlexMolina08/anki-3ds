// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

// Execute the real DNS loop with deterministic sockets and a virtual clock.
#include <assert.h>
#include <arpa/inet.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/select.h>
#include <sys/socket.h>
#include <unistd.h>
#include <3ds.h>

static uint64_t now;
static int sends, closes;
static int mode;
static uint8_t packet[512];
static size_t packet_size;

#define socket mock_socket
#define connect mock_connect
#define send mock_send
#define recv mock_recv
#define select mock_select
#define fcntl mock_fcntl
#define close mock_close

static int mock_socket(int domain, int type, int protocol) { return 3; }
static int mock_connect(int fd, const struct sockaddr *address, socklen_t length) { return 0; }
static int mock_fcntl(int fd, int command, ...) { return 0; }
static int mock_close(int fd) { closes++; return 0; }
static ssize_t mock_send(int fd, const void *data, size_t size, int flags) {
    sends++;
    memcpy(packet, data, size);
    packet_size = size;
    if (mode == 3 && sends == 2) return -1;
    return size;
}
static int mock_select(int nfds, fd_set *reads, fd_set *writes, fd_set *errors, struct timeval *timeout) {
    now += timeout->tv_sec * 1000 + timeout->tv_usec / 1000;
    return mode == 0 && sends == 2;
}
static ssize_t mock_recv(int fd, void *data, size_t size, int flags) {
    uint8_t *response = data;
    memcpy(response, packet, packet_size);
    response[2] = 0x81; response[3] = 0x80; response[7] = 1;
    const uint8_t answer[] = {0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 192, 0, 2, 1};
    memcpy(response + packet_size, answer, sizeof(answer));
    return packet_size + sizeof(answer);
}

uint64_t osGetTime(void) { return now; }
Result PS_GenerateRandomBytes(void *data, size_t size) { memset(data, 17, size); return 0; }
int SOCU_GetNetworkOpt(int level, int option, void *data, socklen_t *size) {
    SOCU_DNSTableEntry server = {0};
    server.family = AF_INET;
    server.ip.s_addr = htonl(0xc0000235);
    memcpy(data, &server, sizeof(server));
    *size = sizeof(server);
    return 0;
}

#include "../sync-transport/c/dns.c"

static bool cancel(void *context) { return mode == 2 && now >= 1000; }
int main(void) {
    char ip[16], error[160];
    for (mode = 0; mode < 4; mode++) {
        now = 0; sends = 0; closes = 0;
        int result = probe_dns_with_context("sync.ankiweb.net", ip, error, cancel, NULL, false);
        assert(closes == 1);
        if (mode == 0) {
            assert(result == 0 && sends == 2 && now >= 2500 && now < 5000);
            assert(!strcmp(ip, "192.0.2.1"));
        } else if (mode == 1) {
            assert(result != 0 && sends == 2 && now == 5000);
            assert(strstr(error, "timeout"));
        } else if (mode == 2) {
            assert(result != 0 && sends == 1 && now <= 1100);
            assert(strstr(error, "cancelled"));
        } else {
            assert(result != 0 && sends == 2 && now == 2500);
            assert(strstr(error, "retry failed"));
        }
    }
    puts("PASS: DNS retransmit recovery, fixed deadline, cancellation, send failure");
}
