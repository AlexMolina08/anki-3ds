// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

#include "https_client.h"
#include <3ds.h>
#include <curl/curl.h>
#include <malloc.h>
#include <stdio.h>
#include <string.h>
#include <strings.h>
#include <stdlib.h>
#include "dns.h"

#define CA_FILE "sdmc:/3ds/anki-3ds/cacert.pem"
#define MAX_RESPONSE_BYTES (8u * 1024u * 1024u)

struct ResponseBuffer {
    uint8_t *data;
    size_t size;
    size_t capacity;
    uint64_t deadline;
    uint32_t io_timeout_ms;
    bool timed_out;
    curl_off_t last_download;
    curl_off_t last_upload;
    uint32_t sampled_heap_peak;
    FILE *file;
    const Anki3dsRequestOptions *options;
};

static size_t header_cb(char *data, size_t size, size_t count, void *context) {
    if (size && count > SIZE_MAX / size) return 0;
    size_t bytes = size * count;
    Anki3dsHttpResponse *response = context;
    if (bytes >= 5 && !strncmp(data, "HTTP/", 5)) response->server_time = 0;
    if (bytes > 5 && bytes < 128 && !strncasecmp(data, "Date:", 5)) {
        char value[128];
        memcpy(value, data + 5, bytes - 5);
        value[bytes - 5] = 0;
        time_t date = curl_getdate(value, NULL);
        if (date > 0) response->server_time = (int64_t)date;
    }
    return bytes;
}

static size_t write_cb(char *ptr, size_t size, size_t nmemb, void *userdata) {
    if (size && nmemb > SIZE_MAX / size) return 0;
    size_t bytes = size * nmemb;
    struct ResponseBuffer *buf = (struct ResponseBuffer *)userdata;
    
    if (buf->file) {
        if (bytes > buf->options->max_file_bytes - buf->size) return 0;
        size_t written = fwrite(ptr, 1, bytes, buf->file);
        buf->size += written;
        return written;
    }
    if (bytes > MAX_RESPONSE_BYTES - buf->size) return 0;
    if (buf->size + bytes > buf->capacity) {
        size_t new_cap = (buf->capacity == 0) ? 16384 : buf->capacity * 2;
        while (new_cap < buf->size + bytes) {
            new_cap *= 2;
        }
        uint8_t *new_ptr = realloc(buf->data, new_cap);
        if (!new_ptr) return 0;
        buf->data = new_ptr;
        buf->capacity = new_cap;
    }
    
    memcpy(buf->data + buf->size, ptr, bytes);
    buf->size += bytes;
    uint32_t heap = (uint32_t)mallinfo().uordblks;
    if (heap > buf->sampled_heap_peak) buf->sampled_heap_peak = heap;
    return bytes;
}

static int xfer_cb(void *clientp, curl_off_t dltotal, curl_off_t dlnow, curl_off_t ultotal, curl_off_t ulnow) {
    (void)dltotal; (void)dlnow; (void)ultotal; (void)ulnow;
    struct ResponseBuffer *buf = (struct ResponseBuffer *)clientp;
    uint32_t heap = (uint32_t)mallinfo().uordblks;
    if (heap > buf->sampled_heap_peak) buf->sampled_heap_peak = heap;
    if (buf->options->cancelled && buf->options->cancelled(buf->options->context)) return 1;
    if (buf->options->progress && buf->options->progress(buf->options->context,
        (uint64_t)(dlnow + ulnow), (uint64_t)(dltotal + ultotal))) return 1;
    if (dlnow != buf->last_download || ulnow != buf->last_upload) {
        buf->last_download = dlnow; buf->last_upload = ulnow;
        buf->deadline = osGetTime() + buf->io_timeout_ms;
    }
    if (osGetTime() >= buf->deadline) {
        buf->timed_out = true;
        return 1; // abort transfer
    }
    return 0;
}

static bool dns_cancel(void *context) {
    const Anki3dsRequestOptions *options = context;
    return options->cancelled && options->cancelled(options->context);
}

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
) {
    if (!out_resp) return -1;
    memset(out_resp, 0, sizeof(*out_resp));
    
    char ip[16] = {0};
    char dns_error[160] = {0};
    
    bool allow_override = false;
#ifdef ANKI_3DS_EMULATOR
    allow_override = true;
#endif
    int dns_rc = probe_dns_with_context(host, ip, dns_error, dns_cancel, (void *)options, allow_override);
    if (dns_rc != 0) {
        snprintf(out_resp->error_msg, sizeof(out_resp->error_msg), "DNS failed: %s", dns_error);
        return -2;
    }
    
    CURL *easy = curl_easy_init();
    if (!easy) {
        snprintf(out_resp->error_msg, sizeof(out_resp->error_msg), "curl_easy_init failed");
        return -3;
    }
    
    char url[512];
    snprintf(url, sizeof(url), "https://%s:%u%s", host, options->port, path);
    
    char mapping[256];
    snprintf(mapping, sizeof(mapping), "%s:%u:%s", host, options->port, ip);
    
    struct curl_slist *resolve = curl_slist_append(NULL, mapping);
    struct curl_slist *headers = curl_slist_append(NULL, "Content-Type: application/octet-stream");
    if (header_anki_sync && header_anki_sync[0]) {
        char sync_hdr_line[512];
        snprintf(sync_hdr_line, sizeof(sync_hdr_line), "anki-sync: %s", header_anki_sync);
        headers = curl_slist_append(headers, sync_hdr_line);
    }
    
    struct ResponseBuffer rx_buf = {0};
    rx_buf.options = options;
    rx_buf.io_timeout_ms = timeout_ms;
    rx_buf.deadline = osGetTime() + (uint64_t)timeout_ms;
    
    const char *ca_path = options->ca_file;
    FILE *upload = NULL;
    if (options->upload_file) {
        upload = fopen(options->upload_file, "rb");
        if (!upload) { curl_easy_cleanup(easy); curl_slist_free_all(headers); curl_slist_free_all(resolve); return -4; }
        if (fseek(upload, 0, SEEK_END) || ftell(upload) < 0) {
            fclose(upload); curl_easy_cleanup(easy); curl_slist_free_all(headers); curl_slist_free_all(resolve);
            snprintf(out_resp->error_msg, sizeof(out_resp->error_msg), "cannot determine upload size"); return -4;
        }
        body_len = (size_t)ftell(upload);
        if (body_len > options->max_file_bytes) {
            fclose(upload); curl_easy_cleanup(easy); curl_slist_free_all(headers); curl_slist_free_all(resolve);
            snprintf(out_resp->error_msg, sizeof(out_resp->error_msg), "compressed upload exceeds protocol limit"); return -4;
        }
        rewind(upload);
    }
    if (options->download_file) {
        rx_buf.file = fopen(options->download_file, "wb");
        if (!rx_buf.file) {
            if (upload) fclose(upload);
            curl_easy_cleanup(easy); curl_slist_free_all(headers); curl_slist_free_all(resolve); return -4;
        }
    }

    curl_easy_setopt(easy, CURLOPT_URL, url);
    curl_easy_setopt(easy, CURLOPT_HEADERFUNCTION, header_cb);
    curl_easy_setopt(easy, CURLOPT_HEADERDATA, out_resp);
    curl_easy_setopt(easy, CURLOPT_RESOLVE, resolve);
    curl_easy_setopt(easy, CURLOPT_PROTOCOLS_STR, "https");
    curl_easy_setopt(easy, CURLOPT_REDIR_PROTOCOLS_STR, "https");
    curl_easy_setopt(easy, CURLOPT_FOLLOWLOCATION, 0L);
    curl_easy_setopt(easy, CURLOPT_HTTP_VERSION, (long)CURL_HTTP_VERSION_1_1);
    curl_easy_setopt(easy, CURLOPT_SSLVERSION, (long)CURL_SSLVERSION_TLSv1_2);
    curl_easy_setopt(easy, CURLOPT_SSL_VERIFYPEER, 1L);
    curl_easy_setopt(easy, CURLOPT_SSL_VERIFYHOST, 2L);
    curl_easy_setopt(easy, CURLOPT_CAINFO, ca_path);
    curl_easy_setopt(easy, CURLOPT_CONNECTTIMEOUT_MS, (long)connect_timeout_ms);
    curl_easy_setopt(easy, CURLOPT_TIMEOUT_MS, 0L);
    curl_easy_setopt(easy, CURLOPT_NOSIGNAL, 1L);
    curl_easy_setopt(easy, CURLOPT_BUFFERSIZE, 8192L);
    curl_easy_setopt(easy, CURLOPT_WRITEFUNCTION, write_cb);
    curl_easy_setopt(easy, CURLOPT_WRITEDATA, &rx_buf);
    curl_easy_setopt(easy, CURLOPT_NOPROGRESS, 0L);
    curl_easy_setopt(easy, CURLOPT_XFERINFOFUNCTION, xfer_cb);
    curl_easy_setopt(easy, CURLOPT_XFERINFODATA, &rx_buf);
    curl_easy_setopt(easy, CURLOPT_HTTPHEADER, headers);
    curl_easy_setopt(easy, CURLOPT_USERAGENT, "Anki3DS/1.0");
    
    if (upload) {
        curl_easy_setopt(easy, CURLOPT_POST, 1L);
        curl_easy_setopt(easy, CURLOPT_READDATA, upload);
        curl_easy_setopt(easy, CURLOPT_POSTFIELDSIZE_LARGE, (curl_off_t)body_len);
    } else if (body && body_len > 0) {
        curl_easy_setopt(easy, CURLOPT_POST, 1L);
        curl_easy_setopt(easy, CURLOPT_POSTFIELDS, (const char *)body);
        curl_easy_setopt(easy, CURLOPT_POSTFIELDSIZE_LARGE, (curl_off_t)body_len);
    } else {
        curl_easy_setopt(easy, CURLOPT_POST, 1L);
        curl_easy_setopt(easy, CURLOPT_POSTFIELDSIZE_LARGE, (curl_off_t)0);
    }
    
    char errbuf[CURL_ERROR_SIZE] = {0};
    curl_easy_setopt(easy, CURLOPT_ERRORBUFFER, errbuf);
    
    CURLcode code = curl_easy_perform(easy);
    if (rx_buf.timed_out) {
        code = CURLE_OPERATION_TIMEDOUT;
        snprintf(errbuf, sizeof(errbuf), "sync I/O timeout");
    }
    out_resp->curl_code = (int)code;
    
    long http_code = 0;
    curl_easy_getinfo(easy, CURLINFO_RESPONSE_CODE, &http_code);
    out_resp->http_status = (int)http_code;
    
    char *redirect_url = NULL;
    curl_easy_getinfo(easy, CURLINFO_REDIRECT_URL, &redirect_url);
    if (redirect_url) {
        snprintf(out_resp->redirect_url, sizeof(out_resp->redirect_url), "%s", redirect_url);
    }
    
    if (code != CURLE_OK) {
        snprintf(out_resp->error_msg, sizeof(out_resp->error_msg), "%s", errbuf[0] ? errbuf : curl_easy_strerror(code));
    }
    
    out_resp->response_body = rx_buf.data;
    out_resp->response_body_len = rx_buf.file ? 0 : rx_buf.size;
    out_resp->response_capacity_peak = rx_buf.capacity;
    out_resp->sampled_heap_peak = rx_buf.sampled_heap_peak;
    
    if (upload) fclose(upload);
    if (rx_buf.file) {
        int flush_result = fflush(rx_buf.file);
        int close_result = fclose(rx_buf.file);
        if (flush_result != 0 || close_result != 0) {
            if (code == CURLE_OK) { code = CURLE_WRITE_ERROR; out_resp->curl_code = code; }
        }
    }
    curl_slist_free_all(headers);
    curl_slist_free_all(resolve);
    curl_easy_cleanup(easy);
    
    return (code == CURLE_OK) ? 0 : (int)code;
}

void anki_3ds_http_response_free(Anki3dsHttpResponse *resp) {
    if (resp && resp->response_body) {
        free(resp->response_body);
        resp->response_body = NULL;
        resp->response_body_len = 0;
    }
}

uint32_t anki_3ds_transport_heap_used(void) { return (uint32_t)mallinfo().uordblks; }
