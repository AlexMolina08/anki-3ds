// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

#ifndef ANKI_3DS_HTTPS_CLIENT_H
#define ANKI_3DS_HTTPS_CLIENT_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    int http_status;
    int curl_code;
    uint8_t *response_body;
    size_t response_body_len;
    char error_msg[256];
    char redirect_url[512];
    size_t response_capacity_peak;
    uint32_t sampled_heap_peak;
    int64_t server_time;
} Anki3dsHttpResponse;

// Performs an HTTPS request over native Horizon sockets with curl+mbedTLS.
// host: domain name (e.g. "sync.ankiweb.net")
// path: request path (e.g. "/sync/meta")
// header_anki_sync: value of anki-sync header, or NULL if none
// body: request body bytes (already zstd compressed if required)
// body_len: length of request body
// timeout_ms: total request deadline in milliseconds
// connect_timeout_ms: TCP connect deadline in milliseconds
// out_resp: pointer to response struct to populate
// Returns 0 on successful HTTP transaction (status > 0), or non-zero on transport error.
typedef struct {
    uint32_t port;
    const char *ca_file;
    const char *upload_file;
    const char *download_file;
    size_t max_file_bytes;
    int (*cancelled)(void *);
    int (*progress)(void *, uint64_t, uint64_t);
    void *context;
} Anki3dsRequestOptions;

int anki_3ds_https_post(
    const char *host,
    const char *path,
    const char *header_anki_sync,
    const uint8_t *body,
    size_t body_len,
    uint32_t timeout_ms,
    uint32_t connect_timeout_ms,
    const Anki3dsRequestOptions *options,
    Anki3dsHttpResponse *out_resp
);

void anki_3ds_http_response_free(Anki3dsHttpResponse *resp);

#ifdef __cplusplus
}
#endif

#endif // ANKI_3DS_HTTPS_CLIENT_H
